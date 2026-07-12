import { useDraggable } from "@dnd-kit/core"
import { CSS } from "@dnd-kit/utilities"
import {
  Ban,
  CheckCircle2,
  Copy,
  ExternalLink,
  MoreHorizontal,
  Play,
  RotateCcw,
  Rows3,
} from "lucide-react"
import { openTicketFile } from "../../api/tickets"
import { Badge } from "../../components/ui/badge"
import { Button } from "../../components/ui/button"
import { Checkbox } from "../../components/ui/checkbox"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../../components/ui/dropdown-menu"
import type { Density } from "../../hooks/use-display-preferences"
import { initials } from "../../lib/format"
import { cn } from "../../lib/utils"
import { statusMeta, type Status, type Ticket } from "../../types/ticket"
import { Priority } from "./priority"

type Props = {
  ticket: Ticket
  density?: Density
  treeIndent?: number
  dragEnabled?: boolean
  selected?: boolean
  onToggle?: (id: string, range: boolean) => void
  onSelect: (id: string) => void
  onUpdate: (id: string, body: Partial<Ticket>) => Promise<void>
}

export function TicketRow({
  ticket,
  density = "comfortable",
  treeIndent = 0,
  dragEnabled = false,
  selected = false,
  onToggle,
  onSelect,
  onUpdate,
}: Props) {
  const { attributes, isDragging, listeners, setNodeRef, transform } = useDraggable({
    id: ticket.id,
    data: { ticket },
    disabled: !dragEnabled,
    attributes: { role: "link", roleDescription: "draggable ticket" },
  })
  const meta = statusMeta[ticket.status]
  const StatusIcon = meta.icon

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.target === event.currentTarget && event.key === "Enter") {
      event.preventDefault()
      onSelect(ticket.id)
      return
    }
    listeners?.onKeyDown?.(event)
  }

  return (
    <div
      ref={setNodeRef}
      data-ticket-row={ticket.id}
      data-ticket-id={ticket.id}
      {...attributes}
      {...listeners}
      aria-label={`${ticket.id}: ${ticket.title}`}
      className={cn(
        "group grid cursor-pointer grid-cols-[20px_26px_40px_88px_minmax(140px,1fr)_28px] items-center gap-2 border-t px-3 text-xs transition-[background-color,opacity] first:border-t-0 hover:bg-muted/55 focus-visible:z-[1] focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring sm:grid-cols-[20px_26px_40px_108px_minmax(180px,1fr)_60px_26px_28px] lg:grid-cols-[20px_26px_40px_108px_minmax(180px,1fr)_minmax(0,auto)_60px_26px_28px]",
        selected && "bg-primary/8",
        isDragging && "z-20 opacity-40",
        density === "compact" ? "min-h-8" : "min-h-10",
      )}
      style={{
        paddingLeft: 12 + treeIndent,
        transform: CSS.Translate.toString(transform),
      }}
      onClick={(event) => {
        if ((event.target as HTMLElement).closest('[data-slot="checkbox"]')) return
        onSelect(ticket.id)
      }}
      onKeyDown={handleKeyDown}
    >
      <Checkbox
        checked={selected}
        aria-label={`Select ${ticket.id}`}
        className={cn(
          "opacity-0 group-hover:opacity-100 focus-visible:opacity-100",
          selected && "opacity-100",
        )}
        {...(onToggle
          ? {
              onPointerDown: (event: React.PointerEvent) => event.stopPropagation(),
              onClick: (event: React.MouseEvent) => {
                event.stopPropagation()
                onToggle(ticket.id, event.shiftKey)
              },
              onKeyDown: (event: React.KeyboardEvent) => event.stopPropagation(),
            }
          : { disabled: true })}
      />
      <StatusMenu ticket={ticket} onUpdate={onUpdate}>
        <StatusIcon className="size-4" color={meta.color} />
      </StatusMenu>
      <Priority priority={ticket.priority} />
      <span className="truncate font-mono text-[11px] text-muted-foreground">{ticket.id}</span>
      <span className="truncate font-medium text-foreground/90">{ticket.title}</span>
      <TicketLabels ticket={ticket} />
      <span className="hidden whitespace-nowrap text-right text-[11px] text-muted-foreground sm:block">
        {new Intl.DateTimeFormat(undefined, {
          month: "short",
          day: "numeric",
        }).format(new Date(ticket.created))}
      </span>
      <span className="grid size-6 place-items-center rounded-full border bg-muted text-[9px] font-medium text-muted-foreground">
        {initials(ticket.assignee)}
      </span>
      <TicketActions ticket={ticket} onSelect={onSelect} onUpdate={onUpdate} />
    </div>
  )
}

function StatusMenu({
  ticket,
  onUpdate,
  children,
}: Pick<Props, "ticket" | "onUpdate"> & { children: React.ReactNode }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Change status"
            onClick={(event) => event.stopPropagation()}
          />
        }
      >
        {children}
      </DropdownMenuTrigger>
      <DropdownMenuContent sideOffset={5}>
        {(Object.keys(statusMeta) as Status[]).map((status) => {
          const Icon = statusMeta[status].icon
          return (
            <DropdownMenuItem key={status} onClick={() => void onUpdate(ticket.id, { status })}>
              <Icon />
              {statusMeta[status].label}
            </DropdownMenuItem>
          )
        })}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function TicketLabels({ ticket }: { ticket: Ticket }) {
  return (
    <div className="hidden min-w-0 items-center justify-end gap-1.5 lg:flex">
      {ticket.blocked && (
        <Badge variant="destructive">
          <Ban /> blocked
        </Badge>
      )}
      {ticket.tags.slice(0, 2).map((tag) => (
        <Badge variant="outline" key={tag}>
          {tag}
        </Badge>
      ))}
    </div>
  )
}

function TicketActions({ ticket, onSelect, onUpdate }: Props) {
  const copyPath = () => navigator.clipboard.writeText(ticket.path)
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-sm"
            className="text-muted-foreground opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
            aria-label="Ticket actions"
            onClick={(event) => event.stopPropagation()}
          />
        }
      >
        <MoreHorizontal />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={5}>
        <DropdownMenuLabel className="font-mono">{ticket.id}</DropdownMenuLabel>
        <DropdownMenuItem onClick={() => onSelect(ticket.id)}>
          <Rows3 /> Open details
        </DropdownMenuItem>
        {ticket.status === "open" && (
          <DropdownMenuItem onClick={() => void onUpdate(ticket.id, { status: "in_progress" })}>
            <Play /> Start ticket
          </DropdownMenuItem>
        )}
        {ticket.status !== "closed" ? (
          <DropdownMenuItem onClick={() => void onUpdate(ticket.id, { status: "closed" })}>
            <CheckCircle2 /> Mark done
          </DropdownMenuItem>
        ) : (
          <DropdownMenuItem onClick={() => void onUpdate(ticket.id, { status: "open" })}>
            <RotateCcw /> Reopen ticket
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => void copyPath()}>
          <Copy /> Copy file path
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => void openTicketFile(ticket.id)}>
          <ExternalLink /> Open ticket file
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
