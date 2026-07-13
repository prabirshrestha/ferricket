import { MoreHorizontal } from "lucide-react"
import { cn } from "../../lib/utils"

export function Priority({ priority }: { priority: number }) {
  const heights = ["h-1", "h-1.5", "h-2", "h-2.5"]
  return (
    <span
      className="flex h-[18px] w-9 items-end justify-center gap-0.5 text-muted-foreground"
      title={priority === 4 ? "No priority" : "Priority " + priority}
    >
      {priority < 4 ? (
        Array.from({ length: 4 }, (_, index) => (
          <i
            key={index}
            className={cn(
              "block w-0.5 rounded-sm bg-border",
              heights[index],
              index < 4 - priority && (priority === 0 ? "bg-destructive" : "bg-muted-foreground"),
            )}
          />
        ))
      ) : (
        <MoreHorizontal size={14} />
      )}
    </span>
  )
}
