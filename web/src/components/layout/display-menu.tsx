import { RotateCcw, SlidersHorizontal } from "lucide-react"
import type {
  DisplayPreferences,
  Density,
  GroupBy,
  OrderBy,
} from "../../hooks/use-display-preferences"
import type { View } from "../../types/ticket"
import { Button } from "../ui/button"
import { Switch } from "../ui/switch"
import { Popover, PopoverContent, PopoverHeader, PopoverTitle, PopoverTrigger } from "../ui/popover"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select"
import { Separator } from "../ui/separator"

type Props = {
  view: View
  display: DisplayPreferences
  onChange: (patch: Partial<DisplayPreferences>) => void
}

const groupOptions: Array<[GroupBy, string]> = [
  ["status", "Status"],
  ["priority", "Priority"],
  ["type", "Type"],
  ["assignee", "Assignee"],
  ["none", "No grouping"],
]
const orderOptions: Array<[OrderBy, string]> = [
  ["priority", "Priority"],
  ["created", "Created"],
  ["title", "Title"],
  ["id", "ID"],
]

export function DisplayMenu({ view, display, onChange }: Props) {
  const reset = () =>
    onChange({
      groupBy: "status",
      orderBy: "priority",
      descending: false,
      density: "comfortable",
      showClosed: false,
    })
  return (
    <Popover>
      <PopoverTrigger render={<Button variant="ghost" size="sm" />}>
        <SlidersHorizontal /> Display
      </PopoverTrigger>
      <PopoverContent className="w-60 gap-0 p-1.5" align="end" sideOffset={7}>
        <PopoverHeader className="flex-row items-center px-2 py-1.5">
          <PopoverTitle className="text-xs font-medium">Display</PopoverTitle>
          <Button
            className="ml-auto text-muted-foreground"
            variant="ghost"
            size="icon-xs"
            aria-label="Reset display"
            title="Reset display"
            onClick={reset}
          >
            <RotateCcw />
          </Button>
        </PopoverHeader>
        <Separator className="my-1" />
        <div className="grid">
          {display.layout === "grouped" && (
            <DisplaySelect
              label="Grouping"
              value={display.groupBy}
              options={groupOptions}
              onChange={(value) => onChange({ groupBy: value as GroupBy })}
            />
          )}
          <DisplaySelect
            label="Ordering"
            value={display.orderBy}
            options={orderOptions}
            onChange={(value) => onChange({ orderBy: value as OrderBy })}
          />
          <DisplaySelect
            label="Density"
            value={display.density}
            options={[
              ["comfortable", "Comfortable"],
              ["compact", "Compact"],
            ]}
            onChange={(value) => onChange({ density: value as Density })}
          />
          <DisplaySwitch
            label="Reverse order"
            checked={display.descending}
            onChange={(descending) => onChange({ descending })}
          />
          {view === "active" && (
            <DisplaySwitch
              label="Show completed"
              checked={display.showClosed}
              onChange={(showClosed) => onChange({ showClosed })}
            />
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function DisplaySelect({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: string
  options: Array<readonly [string, string]>
  onChange: (value: string) => void
}) {
  const selected = options.find((option) => option[0] === value)?.[1] ?? value
  return (
    <div className="grid min-h-8 grid-cols-[76px_minmax(0,1fr)] items-center gap-2 rounded-md px-2 hover:bg-muted/50">
      <span className="text-xs text-foreground/80">{label}</span>
      <Select value={value} onValueChange={(next) => next && onChange(next)}>
        <SelectTrigger
          size="sm"
          className="w-full border-transparent bg-transparent px-1.5 shadow-none hover:bg-muted"
        >
          <SelectValue>{selected}</SelectValue>
        </SelectTrigger>
        <SelectContent size="compact" align="end">
          {options.map(([option, text]) => (
            <SelectItem key={option} value={option}>
              {text}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}

function DisplaySwitch({
  label,
  checked,
  onChange,
}: {
  label: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <label className="flex min-h-8 cursor-pointer items-center rounded-md px-2 text-xs hover:bg-muted/50">
      <span className="text-foreground/80">{label}</span>
      <Switch className="ml-auto" checked={checked} onCheckedChange={onChange} />
    </label>
  )
}
