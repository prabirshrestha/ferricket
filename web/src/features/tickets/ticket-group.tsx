import { useDroppable } from "@dnd-kit/core"
import { ChevronDown, Plus } from "lucide-react"
import { useState } from "react"
import { Button } from "../../components/ui/button"
import type { Density } from "../../hooks/use-display-preferences"
import { cn } from "../../lib/utils"
import { statusMeta, type Status, type Ticket } from "../../types/ticket"
import { statusDropId } from "./ticket-dnd"
import { TicketRow } from "./ticket-row"

type Props = {
  status: Status
  dropStatus?: Status
  label?: string
  tickets: Ticket[]
  density: Density
  selectedIds?: Set<string>
  onToggle?: (id: string, range: boolean) => void
  onSelect: (id: string) => void
  onUpdate: (id: string, body: Partial<Ticket>) => Promise<void>
  onCreate: () => void
}

export function TicketGroup({
  status,
  dropStatus,
  label,
  tickets,
  density,
  selectedIds,
  onToggle,
  onSelect,
  onUpdate,
  onCreate,
}: Props) {
  const [collapsed, setCollapsed] = useState(false)
  const meta = statusMeta[status]
  const StatusIcon = meta.icon
  const { isOver, setNodeRef } = useDroppable({
    id: statusDropId(dropStatus ?? status),
    data: dropStatus ? { status: dropStatus } : undefined,
    disabled: !dropStatus,
  })
  const name = label ?? meta.label

  return (
    <section
      ref={setNodeRef}
      className={cn("border-b transition-colors", isOver && "bg-primary/8")}
    >
      <header
        className={cn(
          "sticky top-[88px] z-[5] flex h-9 w-full items-center border-y bg-background/95 px-3 text-muted-foreground backdrop-blur-xl",
          isOver && "border-primary/40 bg-primary/10",
        )}
      >
        <Button
          variant="ghost"
          size="sm"
          className="justify-start gap-2 px-1"
          aria-expanded={!collapsed}
          onClick={() => setCollapsed((value) => !value)}
        >
          <ChevronDown
            size={14}
            className={cn("transition-transform", collapsed && "-rotate-90")}
          />
          <StatusIcon size={15} color={meta.color} />
          <strong className="text-[11px] font-medium text-foreground/80">{name}</strong>
          <span className="text-[10px]">{tickets.length}</span>
        </Button>
        <Button
          variant="ghost"
          size="icon-xs"
          className="ml-auto"
          aria-label={`Add ${name} ticket`}
          onClick={onCreate}
        >
          <Plus />
        </Button>
      </header>
      {!collapsed && (
        <div>
          {[...tickets]
            .sort((a, b) => a.priority - b.priority || a.id.localeCompare(b.id))
            .map((ticket) => (
              <TicketRow
                key={ticket.id}
                ticket={ticket}
                dragEnabled={Boolean(dropStatus)}
                density={density}
                selected={selectedIds?.has(ticket.id)}
                onToggle={onToggle}
                onSelect={onSelect}
                onUpdate={onUpdate}
              />
            ))}
        </div>
      )}
    </section>
  )
}
