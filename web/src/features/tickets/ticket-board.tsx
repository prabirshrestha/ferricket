import { useDraggable, useDroppable } from "@dnd-kit/core"
import { CSS } from "@dnd-kit/utilities"
import { Ban, MoreHorizontal, Plus } from "lucide-react"
import { Badge } from "../../components/ui/badge"
import { Button } from "../../components/ui/button"
import { Checkbox } from "../../components/ui/checkbox"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../../components/ui/dropdown-menu"
import type { Density } from "../../hooks/use-display-preferences"
import { initials } from "../../lib/format"
import { cn } from "../../lib/utils"
import { statusMeta, type Status, type Ticket } from "../../types/ticket"
import { Priority } from "./priority"
import { statusDropId } from "./ticket-dnd"

const statuses: Status[] = ["open", "in_progress", "closed"]

type Props = {
  tickets: Ticket[]
  density: Density
  selectedIds: Set<string>
  onToggle: (id: string, range: boolean) => void
  onSelect: (id: string) => void
  onUpdate: (id: string, body: Partial<Ticket>) => Promise<void>
  onCreate: (status: Status) => void
}

export function TicketBoard(props: Props) {
  return (
    <div className="flex h-full snap-x snap-mandatory gap-px overflow-x-auto bg-border">
      {statuses.map((status) => (
        <BoardColumn key={status} status={status} {...props} />
      ))}
    </div>
  )
}

function BoardColumn({ status, tickets, ...props }: Props & { status: Status }) {
  const meta = statusMeta[status]
  const StatusIcon = meta.icon
  const items = tickets.filter((ticket) => ticket.status === status)
  const { isOver, setNodeRef } = useDroppable({
    id: statusDropId(status),
    data: { status },
  })

  return (
    <section
      ref={setNodeRef}
      className={cn(
        "flex min-w-0 flex-[0_0_100%] snap-start flex-col overflow-hidden bg-background transition-colors md:flex-basis-[50%] xl:flex-1 xl:basis-0",
        isOver && "bg-primary/7",
      )}
    >
      <header
        className={cn(
          "flex h-10 shrink-0 items-center gap-2 border-b bg-background px-3",
          isOver && "border-primary/40 bg-primary/10",
        )}
      >
        <StatusIcon className="size-4" color={meta.color} />
        <h2 className="text-xs font-semibold">{meta.label}</h2>
        <span className="text-[11px] text-muted-foreground">{items.length}</span>
        <Button
          variant="ghost"
          size="icon-xs"
          className="ml-auto text-muted-foreground"
          aria-label={`Add ${meta.label} ticket`}
          onClick={() => props.onCreate(status)}
        >
          <Plus />
        </Button>
      </header>
      <div
        className={cn(
          "grid min-h-0 flex-1 content-start overflow-y-auto p-2",
          props.density === "compact" ? "gap-1.5" : "gap-2",
        )}
      >
        {items.map((ticket) => (
          <BoardCard key={ticket.id} ticket={ticket} {...props} />
        ))}
        {items.length === 0 && (
          <div className="rounded-md border border-dashed px-3 py-8 text-center text-xs text-muted-foreground">
            Drop a ticket here
          </div>
        )}
      </div>
    </section>
  )
}

function BoardCard({
  ticket,
  density,
  selectedIds,
  onToggle,
  onSelect,
  onUpdate,
}: Omit<Props, "tickets"> & { ticket: Ticket }) {
  const { attributes, isDragging, listeners, setNodeRef, transform } = useDraggable({
    id: ticket.id,
    data: { ticket },
    attributes: { role: "link", roleDescription: "draggable ticket card" },
  })

  return (
    <article
      ref={setNodeRef}
      data-ticket-id={ticket.id}
      {...attributes}
      {...listeners}
      aria-label={`${ticket.id}: ${ticket.title}`}
      className={cn(
        "group min-w-0 rounded-md border bg-card transition-[border-color,opacity] hover:border-ring/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        isDragging && "z-20 opacity-40",
        density === "compact" ? "p-2.5" : "p-3",
      )}
      style={{ transform: CSS.Translate.toString(transform) }}
      onClick={(event) => {
        if ((event.target as HTMLElement).closest('[data-slot="checkbox"]')) return
        if (event.shiftKey) onToggle(ticket.id, true)
        else onSelect(ticket.id)
      }}
      onKeyDown={(event) => {
        if (event.target === event.currentTarget && event.key === "Enter") {
          event.preventDefault()
          onSelect(ticket.id)
          return
        }
        listeners?.onKeyDown?.(event)
      }}
    >
      <BoardCardHeader
        ticket={ticket}
        selected={selectedIds.has(ticket.id)}
        onToggle={onToggle}
        onUpdate={onUpdate}
      />
      <h3
        className={cn(
          "line-clamp-2 text-sm font-medium leading-5",
          density === "compact" ? "mt-1.5 min-h-5" : "mt-2 min-h-10",
        )}
      >
        {ticket.title}
      </h3>
      <BoardCardMeta ticket={ticket} compact={density === "compact"} />
    </article>
  )
}

function BoardCardHeader({
  ticket,
  selected,
  onToggle,
  onUpdate,
}: {
  ticket: Ticket
  selected: boolean
  onToggle: Props["onToggle"]
  onUpdate: Props["onUpdate"]
}) {
  return (
    <div className="flex items-start gap-2">
      <Checkbox
        checked={selected}
        className={selected ? "opacity-100" : "opacity-0 group-hover:opacity-100"}
        aria-label={`Select ${ticket.id}`}
        onPointerDown={(event) => event.stopPropagation()}
        onClick={(event) => {
          event.stopPropagation()
          onToggle(ticket.id, event.shiftKey)
        }}
        onKeyDown={(event) => event.stopPropagation()}
      />
      <span className="font-mono text-[11px] text-muted-foreground">{ticket.id}</span>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="icon-xs"
              className="ml-auto text-muted-foreground opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
              aria-label="Ticket actions"
              onClick={(event) => event.stopPropagation()}
            />
          }
        >
          <MoreHorizontal />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {statuses.map((status) => (
            <DropdownMenuItem key={status} onClick={() => void onUpdate(ticket.id, { status })}>
              {statusMeta[status].label}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}

function BoardCardMeta({ ticket, compact }: { ticket: Ticket; compact: boolean }) {
  return (
    <div className={cn("flex min-w-0 items-center gap-2", compact ? "mt-2" : "mt-3")}>
      <Priority priority={ticket.priority} />
      {ticket.blocked && <Ban className="size-3.5 shrink-0 text-destructive" />}
      <div className="flex min-w-0 gap-1 overflow-hidden">
        {ticket.tags.slice(0, 2).map((tag) => (
          <Badge className="max-w-24 truncate" key={tag} variant="outline">
            {tag}
          </Badge>
        ))}
      </div>
      <span className="ml-auto grid size-6 shrink-0 place-items-center rounded-full border bg-muted text-[9px] text-muted-foreground">
        {initials(ticket.assignee)}
      </span>
    </div>
  )
}
