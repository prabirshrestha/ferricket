import type { DisplayPreferences, GroupBy } from "../../hooks/use-display-preferences"
import type { Ticket, View } from "../../types/ticket"

export function ticketsForView(tickets: Ticket[], view: View, showClosed: boolean) {
  return tickets.filter(
    (ticket) =>
      view === "all" ||
      (view === "active" && (showClosed || ticket.status !== "closed")) ||
      (view === "blocked" && ticket.blocked && ticket.status !== "closed") ||
      (view === "closed" && ticket.status === "closed"),
  )
}

export function orderTickets(
  tickets: Ticket[],
  display: Pick<DisplayPreferences, "orderBy" | "descending">,
) {
  return [...tickets].sort((a, b) => {
    const comparison =
      display.orderBy === "created"
        ? a.created.localeCompare(b.created)
        : display.orderBy === "title"
          ? a.title.localeCompare(b.title)
          : display.orderBy === "id"
            ? a.id.localeCompare(b.id)
            : a.priority - b.priority
    return display.descending ? -comparison : comparison
  })
}

export function groupTickets(tickets: Ticket[], groupBy: GroupBy) {
  const key = (ticket: Ticket) =>
    groupBy === "priority"
      ? `P${ticket.priority}`
      : groupBy === "type"
        ? ticket.type || "Task"
        : groupBy === "assignee"
          ? ticket.assignee || "Unassigned"
          : groupBy === "none"
            ? "Tickets"
            : ticket.status
  const labels =
    groupBy === "status" ? ["open", "in_progress", "closed"] : [...new Set(tickets.map(key))]
  return labels.map((label) => ({
    label,
    tickets: tickets.filter((ticket) => key(ticket) === label),
  }))
}
