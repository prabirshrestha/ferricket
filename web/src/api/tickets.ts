import type { Meta, Ticket } from "../types/ticket"
import { request } from "./client"

export function fetchTickets() {
  return request<Ticket[]>("/api/tickets")
}
export function fetchMeta() {
  return request<Meta>("/api/meta")
}
export function createTicket(
  body: Pick<Ticket, "title" | "priority"> & {
    description: string
    type?: string
    status?: Ticket["status"]
    assignee?: string
    tags?: string[]
    parent?: string
  },
) {
  return request<Ticket>("/api/tickets", { method: "POST", body: JSON.stringify(body) })
}
export function updateTicket(id: string, body: Partial<Ticket>) {
  const { revision, ...patch } = body
  if (!revision) throw new Error("A ticket revision is required to save changes.")
  return request<Ticket>("/api/tickets/" + id, {
    method: "PATCH",
    headers: { "If-Match": `"${revision}"` },
    body: JSON.stringify(patch),
  })
}
export function addNote(id: string, revision: string, text: string) {
  return request<Ticket>("/api/tickets/" + id + "/notes", {
    method: "POST",
    headers: { "If-Match": `"${revision}"` },
    body: JSON.stringify({ text }),
  })
}

export type Attachment = {
  name: string
  url: string
  markdown: string
  media_type: string
  size: number
}

export async function uploadAttachment(id: string, file: File) {
  const content = await fileAsBase64(file)
  return request<Attachment>(`/api/tickets/${encodeURIComponent(id)}/attachments`, {
    method: "POST",
    body: JSON.stringify({ name: file.name, content }),
  })
}

export type GitHubReference = {
  number: number
  title: string
  url: string
  state: string
  kind: "issue" | "pull_request"
}

export type GitHubReferenceResponse = {
  available: boolean
  repository?: { name_with_owner: string; url: string; host: string }
  items: GitHubReference[]
  reason?: string
}

export function fetchGitHubReferences(query: string, signal?: AbortSignal) {
  return request<GitHubReferenceResponse>(
    `/api/github/references?q=${encodeURIComponent(query.trim())}`,
    { signal },
  )
}

function fileAsBase64(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader()
    reader.onerror = () => reject(reader.error ?? new Error(`Could not read ${file.name}.`))
    reader.onload = () => {
      const value = String(reader.result ?? "")
      const separator = value.indexOf(",")
      if (separator < 0) reject(new Error(`Could not encode ${file.name}.`))
      else resolve(value.slice(separator + 1))
    }
    reader.readAsDataURL(file)
  })
}

export type BulkResult = {
  updated: Ticket[]
  conflicts: Array<{ id: string; message: string; latest?: Ticket }>
}
export function bulkUpdate(
  tickets: Array<Pick<Ticket, "id" | "revision">>,
  patch: Partial<Ticket>,
) {
  return request<BulkResult>("/api/tickets/bulk", {
    method: "PATCH",
    headers: {
      "If-Match": tickets.map(({ revision }) => `\"${revision}\"`).join(", "),
    },
    body: JSON.stringify({ tickets, patch }),
  })
}

export async function openTicketFile(id: string) {
  const response = await fetch("/api/tickets/" + id + "/open", { method: "POST" })
  if (!response.ok) {
    const result = await response.json().catch(() => ({ error: response.statusText }))
    throw new Error(result.error)
  }
}
