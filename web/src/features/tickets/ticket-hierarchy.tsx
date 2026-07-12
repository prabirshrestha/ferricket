import { useMemo, useState } from "react"
import { ChevronDown, ListTree } from "lucide-react"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "../../components/ui/collapsible"
import { cn } from "../../lib/utils"
import type { Density } from "../../hooks/use-display-preferences"
import type { Ticket } from "../../types/ticket"
import { TicketRow } from "./ticket-row"

type Props = {
  tickets: Ticket[]
  density: Density
  selectedIds?: Set<string>
  onToggle?: (id: string, range: boolean) => void
  onSelect: (id: string) => void
  onUpdate: (id: string, body: Partial<Ticket>) => Promise<void>
}
type TreeTicket = { ticket: Ticket; children: TreeTicket[]; detached?: boolean }

export function withAncestorContext(matched: Ticket[], all: Ticket[]): Ticket[] {
  const byId = new Map(all.map((ticket) => [ticket.id, ticket]))
  const included = new Map(matched.map((ticket) => [ticket.id, ticket]))
  for (const ticket of matched) {
    let parent = ticket.parent
    const path = new Set<string>()
    while (parent && !path.has(parent)) {
      path.add(parent)
      const candidate = byId.get(parent)
      if (!candidate) break
      included.set(candidate.id, candidate)
      parent = candidate.parent
    }
  }
  return [...included.values()]
}

export function TicketHierarchy({
  tickets,
  density,
  selectedIds,
  onToggle,
  onSelect,
  onUpdate,
}: Props) {
  const tree = useMemo(() => buildHierarchy(tickets), [tickets])
  return (
    <div className="overflow-hidden rounded-lg border bg-card">
      <div className="flex h-9 items-center gap-2 bg-muted px-3 text-[11px] text-muted-foreground">
        <ListTree size={14} />
        <strong className="font-medium text-foreground/80">Parent & sub-tickets</strong>
        <span>{tickets.length}</span>
      </div>
      {tree.map((node) => (
        <HierarchyNode
          key={node.ticket.id}
          node={node}
          depth={0}
          density={density}
          selectedIds={selectedIds}
          onToggle={onToggle}
          onSelect={onSelect}
          onUpdate={onUpdate}
        />
      ))}
    </div>
  )
}

function HierarchyNode({
  node,
  depth,
  density,
  selectedIds,
  onToggle,
  onSelect,
  onUpdate,
}: { node: TreeTicket; depth: number } & Omit<Props, "tickets">) {
  const [open, setOpen] = useState(true)
  const hasChildren = node.children.length > 0
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="relative" style={{ paddingLeft: `${Math.min(depth, 8) * 18}px` }}>
        {depth > 0 && (
          <span
            className="pointer-events-none absolute top-0 bottom-0 border-l"
            style={{ left: `${Math.min(depth, 8) * 18 - 9}px` }}
          />
        )}
        {hasChildren && (
          <CollapsibleTrigger
            className="absolute top-1/2 z-10 grid size-5 -translate-y-1/2 place-items-center rounded text-muted-foreground hover:bg-accent"
            style={{ left: `${Math.min(depth, 8) * 18 + 3}px` }}
            aria-label={open ? "Collapse sub-tickets" : "Expand sub-tickets"}
          >
            <ChevronDown size={13} className={cn("transition-transform", !open && "-rotate-90")} />
          </CollapsibleTrigger>
        )}
        <TicketRow
          ticket={node.ticket}
          density={density}
          treeIndent={hasChildren ? 24 : 6}
          selected={selectedIds?.has(node.ticket.id)}
          onToggle={onToggle}
          onSelect={onSelect}
          onUpdate={onUpdate}
        />
      </div>
      <CollapsibleContent>
        {node.children.map((child) => (
          <HierarchyNode
            key={child.ticket.id}
            node={child}
            depth={depth + 1}
            density={density}
            selectedIds={selectedIds}
            onToggle={onToggle}
            onSelect={onSelect}
            onUpdate={onUpdate}
          />
        ))}
      </CollapsibleContent>
    </Collapsible>
  )
}

export function buildHierarchy(tickets: Ticket[]): TreeTicket[] {
  const included = new Map(tickets.map((ticket) => [ticket.id, ticket]))
  const children = new Map<string, Ticket[]>()
  const roots: Ticket[] = []
  for (const ticket of tickets) {
    if (ticket.parent && included.has(ticket.parent) && ticket.parent !== ticket.id) {
      children.set(ticket.parent, [...(children.get(ticket.parent) ?? []), ticket])
    } else roots.push(ticket)
  }
  const sort = (items: Ticket[]) =>
    items.sort((a, b) => a.priority - b.priority || a.id.localeCompare(b.id))
  const visited = new Set<string>()
  const expand = (ticket: Ticket, path: Set<string>): TreeTicket => {
    visited.add(ticket.id)
    const nextPath = new Set(path).add(ticket.id)
    return {
      ticket,
      children: sort(children.get(ticket.id) ?? [])
        .filter((child) => !nextPath.has(child.id))
        .map((child) => expand(child, nextPath)),
    }
  }
  const result = sort(roots).map((ticket) => expand(ticket, new Set()))
  for (const ticket of sort([...tickets]))
    if (!visited.has(ticket.id)) result.push(expand(ticket, new Set()))
  return result
}
