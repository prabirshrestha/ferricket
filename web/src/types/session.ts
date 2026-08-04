export type SessionEventKind = "prompt" | "queued" | "steering" | "completed" | "stopped" | "ended"

export type SessionEvent = {
  id: string
  session_id: string
  timestamp: string
  kind: SessionEventKind
  content?: string
  interaction_id?: string
}

export type SessionActivity = {
  events: SessionEvent[]
  warnings: string[]
}
