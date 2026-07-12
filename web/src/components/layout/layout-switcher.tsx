import { ChartNoAxesGantt, GitFork, LayoutDashboard, ListTree, Rows3 } from "lucide-react"
import type { LayoutMode } from "../../hooks/use-display-preferences"
import { ToggleGroup, ToggleGroupItem } from "../ui/toggle-group"
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip"

const layouts = [
  { value: "grouped", label: "List", description: "Grouped list", icon: Rows3 },
  { value: "hierarchy", label: "Tree", description: "Parent hierarchy", icon: ListTree },
  { value: "board", label: "Board", description: "Status board", icon: LayoutDashboard },
  {
    value: "roadmap",
    label: "Roadmap",
    description: "Dependency roadmap",
    icon: ChartNoAxesGantt,
  },
  { value: "dag", label: "DAG", description: "Zoomable dependency DAG", icon: GitFork },
] as const satisfies ReadonlyArray<{
  value: LayoutMode
  label: string
  description: string
  icon: typeof Rows3
}>

export function LayoutSwitcher({
  value,
  onChange,
}: {
  value: LayoutMode
  onChange: (layout: LayoutMode) => void
}) {
  return (
    <ToggleGroup
      aria-label="Ticket layout"
      value={[value]}
      onValueChange={(values) => {
        const layout = values.at(-1)
        if (
          layout === "grouped" ||
          layout === "hierarchy" ||
          layout === "board" ||
          layout === "roadmap" ||
          layout === "dag"
        )
          onChange(layout)
      }}
      variant="outline"
      size="sm"
      spacing={0}
    >
      {layouts.map(({ value: layout, label, description, icon: Icon }) => (
        <Tooltip key={layout}>
          <TooltipTrigger
            render={
              <ToggleGroupItem value={layout} aria-label={description} className="gap-1.5 px-2.5" />
            }
          >
            <Icon />
            <span className="hidden 2xl:inline">{label}</span>
          </TooltipTrigger>
          <TooltipContent>{description}</TooltipContent>
        </Tooltip>
      ))}
    </ToggleGroup>
  )
}
