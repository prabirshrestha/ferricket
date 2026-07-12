import { request } from "./client"

export type PreferenceSection = "workspace" | "display" | "theme"

export function fetchPreferences(section: PreferenceSection) {
  return request<unknown>("/api/preferences/" + section)
}

export function savePreferences(section: PreferenceSection, value: object) {
  return request<unknown>("/api/preferences/" + section, {
    method: "PUT",
    body: JSON.stringify(value),
  })
}
