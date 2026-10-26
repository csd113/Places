-- Places map host API v1. Documentation/type reference, not an executable module.
-- Copy these aliases into a module when using a separate Luau typechecker.
export type State = boolean | number | string
export type Event = {
    subject: string?, kind: string?, key: string?, actor: string?,
    x: number?, y: number?, z: number?, dt: number,
}
export type DoorOperation = "open" | "close" | "toggle" | "lock" | "unlock"
export type Position = { x: number, y: number, z: number }
export type UsePoint = {target: string, approach: {number}, contact: {number}, yaw_degrees: number?, reach: number?, kind: ("interact" | "door" | "chair" | "switch" | "shelf")?, contact_seconds: number?, hold_seconds: number?, clip: string?, socket: string?}
export type TaskDefinition = {task: string, point: ({number} | UsePoint)?, seconds: number?, radius: number?, speed: number?, item: {match: string, value: string}?, destination: UsePoint?, search_radius: number?, target: string?, distance: number?, memory_seconds: number?, steps: {TaskDefinition}?, looped: boolean?, points: {{number}}?, track_player: boolean?, decision_seconds: number?}
export type TaskRequest = {key: string, priority: number, definition: TaskDefinition}
export type SenseRule = {filter: {sense: string, categories: {string}?, radius: number?, player: boolean?, entities: boolean?, objects: {string}?, object_types: {string}?, range: number?, fov_degrees: number?}, response: {response: string, seconds: number?, radius: number?, hops: number?, request: TaskRequest?}}
export type Actor = { x: number, y: number, z: number, illuminated: boolean, intensity: number, player_visible: boolean, shadow_only: boolean, carried_item: string?, retired: boolean, radius: number?, height: number?, ai_owner: string?, ai_task: string?, ai_phase: string?, ai_result: string? }
export type Api = {
    player: () -> Position,
    actor: (id: string) -> Actor,
    held_item: () -> string?,
    handoff: (id: string) -> (),
    release_item: (id: string) -> (),
    animate: (id: string, clip: string, looped: boolean) -> (),
    toggle_animation: (id: string, clip: string) -> (),
    retire_hidden: (id: string) -> (),
    ai_task: (id: string, key: string, request: (TaskRequest | string)?) -> (),
    ai_pause: (id: string, key: string, paused: boolean) -> (),
    ai_senses: (id: string, rules: ({SenseRule} | string)) -> (),
    navigate: (id: string, x: number, y: number, z: number) -> (),
    message: (title: string, lines: {string}) -> (),
    finish: (won: boolean, title: string, detail: string) -> (),
    reset: () -> (),
    change_level: (id: string) -> (),
    exists: (id: string) -> boolean,
    door: (id: string, operation: DoorOperation) -> (),
    door_status: (id: string) -> {phase: string, locked: boolean, obstructed: boolean, angle: number},
    prompt: (id: string, prompt: string) -> (),
    light: (id: string, on: boolean) -> (),
    sound: (id: string, playing: boolean) -> (),
    physics: (group: string, activate: boolean) -> (),
    spawn: (point: string) -> (),
    state: (id: string, key: string) -> State,
    set_state: (id: string, key: string, value: State) -> (),
    notice: (message: string) -> (),
}
export type Handler = (api: Api, event: Event) -> ()
export type Module = {[string]: Handler}
