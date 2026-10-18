-- Places map host API v1. Documentation/type reference, not an executable module.
-- Copy these aliases into a module when using a separate Luau typechecker.
export type State = boolean | number | string
export type Event = {
    subject: string?, kind: string?, key: string?, actor: string?,
    x: number?, y: number?, z: number?, dt: number,
}
export type DoorOperation = "open" | "close" | "toggle" | "lock" | "unlock"
export type Position = { x: number, y: number, z: number }
export type Actor = { x: number, y: number, z: number, illuminated: boolean, intensity: number, player_visible: boolean }
export type Api = {
    player: () -> Position,
    actor: (id: string) -> Actor,
    navigate: (id: string, x: number, y: number, z: number) -> (),
    message: (title: string, lines: {string}) -> (),
    finish: (won: boolean, title: string, detail: string) -> (),
    reset: () -> (),
    change_level: (id: string) -> (),
    exists: (id: string) -> boolean,
    door: (id: string, operation: DoorOperation) -> (),
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
