import type { SessionActivity } from "../types/session"
import { request } from "./client"

export function fetchSessionActivity() {
  return request<SessionActivity>("/api/session-events")
}
