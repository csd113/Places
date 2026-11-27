use super::{
    Grant, Session,
    protocol::{self, Reply},
    transport_windows::{self, Input, Pipe},
};
use crate::editor::agent_api::SceneStamp;
use std::time::{Duration, Instant};

#[test]
fn grant_needs_live_verified_scope_and_revoke_does_not_resume() {
    let scene = SceneStamp::new();
    let other = SceneStamp::new();
    let grant = Grant {
        scope: scene.clone(),
        nonce: [1; 16],
    };
    assert!(
        grant.permits(Some(&scene), true),
        "live project is admitted"
    );
    assert!(
        !grant.permits(Some(&scene), false),
        "lost monitorability denies"
    );
    assert!(
        !grant.permits(None, true),
        "modal, switching or closing denies"
    );
    assert!(
        !grant.permits(Some(&other), true),
        "a new project epoch denies"
    );
    let mut session = Session::default();
    assert!(
        !session.active(),
        "default launch or verification mode is off"
    );
    assert!(
        session.enable(&scene).is_err(),
        "no visibility platform proof cannot opt in"
    );
    assert!(
        !session.retain_scope(Some(&scene)),
        "restoring visibility does not create a grant"
    );
    assert!(!session.pause(), "taking over while off is inert");
    assert!(!session.active(), "a fresh deliberate opt-in is required");
}

#[test]
fn fixed_protocol_rejects_stale_version_and_non_status_messages() {
    let nonce = [7; 16];
    let wire = protocol::request(nonce);
    assert_eq!(protocol::dispatch(&wire, nonce), Reply::Ready);
    assert_eq!(protocol::dispatch(&wire, [8; 16]), Reply::StaleSession);
    assert_eq!(protocol::dispatch(b"PLEA", nonce), Reply::BadMessage);
    let mut version = wire;
    if let Some(byte) = version.get_mut(4) {
        *byte = 2;
    }
    assert_eq!(
        protocol::dispatch(&version, nonce),
        Reply::UnsupportedVersion
    );
    let mut operation = wire;
    if let Some(byte) = operation.get_mut(5) {
        *byte = 2;
    }
    assert_eq!(
        protocol::dispatch(&operation, nonce),
        Reply::UnsupportedOperation
    );
    assert!(
        protocol::decode(r"\\server\pipe\x").is_none(),
        "a client never opens arbitrary paths"
    );
}

#[test]
fn pilot_wire_rejects_unapproved_actions_and_wrong_grant() {
    let nonce = [21; 16];
    let request = |body: &[u8]| protocol::pilot_message(nonce, body);
    let inspect = request(br#"{"op":"inspect","samples":[[0.1,0.2]]}"#);
    assert!(
        matches!(protocol::pilot_call(&inspect, nonce), Some(Ok(super::Call::Inspect { samples })) if samples.len() == 1),
        "a valid bounded query reaches the pilot"
    );
    assert!(
        matches!(
            protocol::pilot_call(&inspect, [22; 16]),
            Some(Err(Reply::StaleSession))
        ),
        "a different grant cannot dispatch a pilot query"
    );
    let raw = request(br#"{"op":"preview","request_id":1,"context":1,"actions":[{"capability":"raw.write","path":"scene.json"}]}"#);
    assert!(
        matches!(
            protocol::pilot_call(&raw, nonce),
            Some(Err(Reply::BadMessage))
        ),
        "no raw-write or generic capability can enter the planner"
    );
    let extra = request(br#"{"op":"inspect","path":"scene.json"}"#);
    assert!(
        matches!(
            protocol::pilot_call(&extra, nonce),
            Some(Err(Reply::BadMessage))
        ),
        "unknown fields are refused before dispatch"
    );
}

fn windows_session() -> Result<u32, String> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let mut session_id = 0;
    // SAFETY: querying the current process and writing one live integer.
    if unsafe { ProcessIdToSessionId(std::process::id(), &raw mut session_id) } == 0_i32 {
        return Err("No test logon session".into());
    }
    Ok(session_id)
}

fn check_live_status(pipe: &mut Pipe, nonce: protocol::Nonce) -> Result<(), String> {
    let _listening = pipe.poll();
    let client = std::thread::spawn(move || {
        let first = transport_windows::probe(nonce)?;
        let adjacent = transport_windows::probe(nonce)?;
        Ok::<_, String>((first, adjacent))
    });
    let deadline = Instant::now();
    let mut reply = false;
    while deadline.elapsed() < Duration::from_secs(2) && !client.is_finished() {
        if let Some(Input::Message(message)) = pipe.poll() {
            reply = pipe.reply(protocol::dispatch(&message, nonce));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(reply, "bounded server admitted the local peer and replied");
    let (first, adjacent) = client
        .join()
        .map_err(|_panic| "Probe thread did not complete")??;
    assert_eq!(first, "Places Editor agent session is active.");
    assert_eq!(
        adjacent, first,
        "a fresh request can follow the previous reply"
    );
    let _disconnected = pipe.poll();
    Ok(())
}

#[test]
fn local_pipe_status_disconnect_and_revoke_close_the_endpoint() -> Result<(), String> {
    let session_id = windows_session()?;
    let nonce = transport_windows::nonce().map_err(|error| error.to_string())?;
    assert!(
        transport_windows::probe(nonce).is_err(),
        "no endpoint exists before consent"
    );
    let mut pipe = Pipe::open(nonce, session_id).map_err(|error| error.to_string())?;
    check_live_status(&mut pipe, nonce)?;
    let scope = SceneStamp::new();
    let mut state = Session {
        live: Some(super::Live {
            grant: Grant {
                scope: scope.clone(),
                nonce,
            },
            pipe,
        }),
        ..Session::default()
    };
    assert!(
        state.retain_scope(Some(&SceneStamp::new())),
        "switch revokes the old endpoint"
    );
    assert!(
        transport_windows::probe(nonce).is_err(),
        "stale endpoint closed"
    );
    assert!(
        !state.retain_scope(Some(&scope)) && !state.active(),
        "restoring old project cannot regrant"
    );
    // Model the loss of a presented/visible frame while a fresh pipe is live.
    let next = transport_windows::nonce().map_err(|error| error.to_string())?;
    let new_pipe = Pipe::open(next, session_id).map_err(|error| error.to_string())?;
    state.live = Some(super::Live {
        grant: Grant {
            scope: scope.clone(),
            nonce: next,
        },
        pipe: new_pipe,
    });
    assert!(
        matches!(state.tick(Some(&scope), false), super::Tick::Paused),
        "loss of a monitorable frame must revoke a live grant"
    );
    assert_eq!(
        state.label(),
        None,
        "a platform with no monitor cannot resume"
    );
    assert!(
        matches!(state.tick(Some(&scope), true), super::Tick::Idle) && !state.active(),
        "restoring a frame cannot restore the previous grant"
    );
    assert!(
        transport_windows::probe(next).is_err(),
        "hidden-view grant stays revoked"
    );
    Ok(())
}

#[test]
fn loss_of_monitorability_requires_fresh_grant_and_take_over_revokes_it() -> Result<(), String> {
    let os_session = windows_session()?;
    let scope = SceneStamp::new();
    let mut state = Session::default();

    // This fixture starts immediately after the desktop has approved an
    // explicit grant. It does not substitute for real desktop verification.
    let install = |test_editor: &mut Session| -> Result<protocol::Nonce, String> {
        let nonce = transport_windows::nonce().map_err(|error| error.to_string())?;
        test_editor.live = Some(super::Live {
            grant: Grant {
                scope: scope.clone(),
                nonce,
            },
            pipe: Pipe::open(nonce, os_session).map_err(|error| error.to_string())?,
        });
        test_editor.paused = false;
        Ok(nonce)
    };

    assert!(!state.active());
    let old = install(&mut state)?;
    assert_eq!(state.label(), Some("Take Over"));
    let live = state.live.as_mut().ok_or("Missing initial grant")?;
    check_live_status(&mut live.pipe, old)?;

    // Simulate monitorability lost through occlusion. The editor's production
    // tick, not the test fixture, must retire and close the active endpoint.
    assert!(matches!(
        state.tick(Some(&scope), false),
        super::Tick::Paused
    ));
    assert!(!state.active());
    assert!(
        transport_windows::probe(old).is_err(),
        "cover revoked the old code"
    );
    assert!(matches!(state.tick(Some(&scope), true), super::Tick::Idle));
    assert!(!state.active());
    assert!(
        transport_windows::probe(old).is_err(),
        "uncovering did not revive it"
    );
    assert!(
        state.enable(&scope).is_err(),
        "missing native proof cannot resume"
    );

    // Model a separate deliberate grant after fresh native approval.
    let fresh = install(&mut state)?;
    assert_ne!(old, fresh);
    assert!(
        transport_windows::probe(old).is_err(),
        "old code stays revoked"
    );
    let fresh_live = state.live.as_mut().ok_or("Missing fresh grant")?;
    check_live_status(&mut fresh_live.pipe, fresh)?;

    assert!(state.pause(), "Take Over retires the new grant");
    assert!(!state.active());
    assert!(
        transport_windows::probe(fresh).is_err(),
        "new endpoint is closed"
    );
    assert!(matches!(state.tick(Some(&scope), true), super::Tick::Idle));
    assert!(transport_windows::probe(fresh).is_err());
    Ok(())
}
