import { CheckCircle2, Circle, CircleDashed, X } from "lucide-react"
import { bulkUpdate } from "../../api/tickets"
import { Button } from "../../components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../components/ui/select"
import { statusMeta, type Status, type Ticket } from "../../types/ticket"

export function BulkActionBar({
  tickets,
  onResult,
  onClear,
}: {
  tickets: Ticket[]
  onResult: (updated: Ticket[], conflicts: string[]) => void
  onClear: () => void
}) {
  const apply = async (patch: Partial<Ticket>) => {
    const result = await bulkUpdate(
      tickets.map(({ id, revision }) => ({ id, revision })),
      patch,
    )
    onResult(
      result.updated,
      result.conflicts.map((item) => item.id),
    )
  }
  return (
    <div className="fixed bottom-6 left-1/2 z-30 flex -translate-x-1/2 items-center gap-2 rounded-xl border bg-popover p-2 shadow-2xl">
      <span className="px-2 text-xs font-medium">{tickets.length} selected</span>
      <Select onValueChange={(value) => value && void apply({ status: value as Status })}>
        <SelectTrigger className="w-36" size="sm">
          <SelectValue>Status</SelectValue>
        </SelectTrigger>
        <SelectContent>
          {(Object.keys(statusMeta) as Status[]).map((status) => {
            const Icon =
              status === "open" ? Circle : status === "in_progress" ? CircleDashed : CheckCircle2
            return (
              <SelectItem key={status} value={status}>
                <Icon />
                {statusMeta[status].label}
              </SelectItem>
            )
          })}
        </SelectContent>
      </Select>
      <Select onValueChange={(value) => value !== null && void apply({ priority: Number(value) })}>
        <SelectTrigger className="w-32" size="sm">
          <SelectValue>Priority</SelectValue>
        </SelectTrigger>
        <SelectContent>
          {[0, 1, 2, 3, 4].map((priority) => (
            <SelectItem key={priority} value={String(priority)}>
              P{priority}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button variant="ghost" size="icon-sm" aria-label="Clear selection" onClick={onClear}>
        <X />
      </Button>
    </div>
  )
}
