import type { View } from "../types/ticket"

const builtInViews = new Set<View>(["active", "all", "blocked", "closed"])

export type AppRoute = {
  view?: View
  savedViewId?: string
  ticketId?: string
  insights?: boolean
}

export function parseAppRoute(pathname: string): AppRoute {
  const parts = pathname.split("/").filter(Boolean).map(decodeRoutePart)
  if (parts[0] === "insights") return { insights: true }
  if (parts[0] === "views" && parts[1]) {
    return { savedViewId: parts[1], ticketId: parts[2] === "ticket" ? parts[3] : undefined }
  }
  if (parts[0] === "ticket" && parts[1]) return { ticketId: parts[1] }
  if (builtInViews.has(parts[0] as View)) {
    return { view: parts[0] as View, ticketId: parts[1] === "ticket" ? parts[2] : undefined }
  }
  return {}
}

function decodeRoutePart(part: string) {
  try {
    return decodeURIComponent(part)
  } catch {
    return part
  }
}

export function viewRoute(view: View) {
  return "/" + view
}
export function insightsRoute() {
  return "/insights"
}
export function savedViewRoute(id: string) {
  return "/views/" + encodeURIComponent(id)
}
export function ticketRoute(base: string, id: string) {
  return base + "/ticket/" + encodeURIComponent(id)
}
