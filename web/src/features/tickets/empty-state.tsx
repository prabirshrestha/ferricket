import { CheckCircle2, ListFilter, Plus, X } from "lucide-react"
import { Button } from "../../components/ui/button"

export function EmptyState({
  onCreate,
  filtered = false,
  onClearFilters,
}: {
  onCreate: () => void
  filtered?: boolean
  onClearFilters?: () => void
}) {
  const Icon = filtered ? ListFilter : CheckCircle2
  return (
    <div className="flex h-96 flex-col items-center justify-center rounded-xl border border-dashed text-muted-foreground">
      <div className="grid size-12 place-items-center rounded-2xl border bg-card text-primary">
        <Icon size={24} />
      </div>
      <h2 className="mt-4 text-sm font-medium text-foreground/80">
        {filtered ? "No tickets match these filters" : "No tickets here"}
      </h2>
      <p className="mt-1 mb-5 text-[11px]">
        {filtered
          ? "Try a broader query or clear the active filters."
          : "Everything is clear, or this view has no work."}
      </p>
      {filtered && onClearFilters ? (
        <Button size="sm" variant="secondary" onClick={onClearFilters}>
          <X /> Clear filters
        </Button>
      ) : (
        <Button size="sm" onClick={onCreate}>
          <Plus /> Create a ticket
        </Button>
      )}
    </div>
  )
}
