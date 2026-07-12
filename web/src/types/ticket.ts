import { Ban, CheckCircle2, Circle, CircleDashed } from "lucide-react"

export type Status = "open" | "in_progress" | "closed"
export type View = "all" | "active" | "blocked" | "closed"

export type Ticket = {
  id: string
  revision: string
  path: string
  status: Status
  deps: string[]
  links: string[]
  created: string
  updated?: string
  type: string
  priority: number
  assignee?: string
  external_ref?: string
  parent?: string
  tags: string[]
  title: string
  description: string
  raw: string
  blocked: boolean
  notes: Array<{ timestamp?: string; text: string }>
}

export type Meta = {
  tickets_dir: string
  workspace: string
  version: string
  watch_enabled: boolean
  current_user?: string
}

export const statusMeta = {
  open: { label: "Todo", icon: Circle, color: "#a3a3a8" },
  in_progress: { label: "In progress", icon: CircleDashed, color: "#f2c94c" },
  closed: { label: "Done", icon: CheckCircle2, color: "#707078" },
} satisfies Record<Status, { label: string; icon: typeof Ban; color: string }>

export const viewLabel: Record<View, string> = {
  all: "All tickets",
  active: "Active",
  blocked: "Blocked",
  closed: "Closed",
}
