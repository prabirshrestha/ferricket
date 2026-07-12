import { useMemo, useState } from "react"
import { Check, CircleDot, Diamond, Flag, Tags, UserRound, Workflow } from "lucide-react"
import { Button } from "../../../components/ui/button"
import { Input } from "../../../components/ui/input"
import { Popover, PopoverContent, PopoverTrigger } from "../../../components/ui/popover"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../../components/ui/select"
import { Separator } from "../../../components/ui/separator"
import { cn } from "../../../lib/utils"
import { statusMeta, type Status } from "../../../types/ticket"
import { labelItems, labelOptions } from "../label-combobox"
import { searchOptionMatches, type SearchOption } from "../search-combobox"

const types = ["task", "bug", "feature", "epic", "chore"]
const priorityLabels = ["Urgent", "High", "Medium", "Low", "No priority"]

type Props = {
  status: Status
  priority: number
  type: string
  assignee: string
  parentId: string
  labels: string[]
  assignees: string[]
  parents: SearchOption[]
  labelSuggestions: string[]
  onStatusChange: (value: Status) => void
  onPriorityChange: (value: number) => void
  onTypeChange: (value: string) => void
  onAssigneeChange: (value: string) => void
  onParentChange: (value: string) => void
  onLabelsChange: (value: string[]) => void
}

export function TicketPropertyToolbar({
  status,
  priority,
  type,
  assignee,
  parentId,
  labels,
  assignees,
  parents,
  labelSuggestions,
  onStatusChange,
  onPriorityChange,
  onTypeChange,
  onAssigneeChange,
  onParentChange,
  onLabelsChange,
}: Props) {
  const StatusIcon = statusMeta[status].icon
  const assigneeOptions = [
    { value: "", label: "Unassigned" },
    ...assignees.map((value) => ({ value, label: value })),
  ]

  return (
    <div className="flex flex-wrap items-center gap-1.5" aria-label="Ticket properties">
      <PropertySelect
        label="Status"
        icon={<StatusIcon style={{ color: statusMeta[status].color }} />}
        value={status}
        display={statusMeta[status].label}
        onChange={(value) => onStatusChange(value as Status)}
        options={(Object.keys(statusMeta) as Status[]).map((value) => [
          value,
          statusMeta[value].label,
        ])}
      />
      <PropertySelect
        label="Priority"
        icon={<Flag />}
        value={String(priority)}
        display={priority === 4 ? "No priority" : priorityLabels[priority]}
        onChange={(value) => onPriorityChange(Number(value))}
        options={priorityLabels.map((label, value) => [String(value), `P${value} · ${label}`])}
      />
      <PropertySelect
        label="Type"
        icon={<Diamond />}
        value={type}
        display={capitalize(type)}
        onChange={onTypeChange}
        options={types.map((value) => [value, capitalize(value)])}
      />
      <SearchableProperty
        label="Assignee"
        icon={<UserRound />}
        value={assignee}
        display={assignee || "Assignee"}
        placeholder="Search assignees…"
        options={assigneeOptions}
        onChange={onAssigneeChange}
      />
      <SearchableProperty
        label="Parent"
        icon={<Workflow />}
        value={parentId}
        display={parentId === "none" ? "Parent" : parentId}
        placeholder="Search by ID or title…"
        options={parents}
        onChange={onParentChange}
      />
      <LabelProperty value={labels} options={labelSuggestions} onChange={onLabelsChange} />
    </div>
  )
}

function PropertySelect({
  label,
  icon,
  value,
  display,
  options,
  onChange,
}: {
  label: string
  icon: React.ReactNode
  value: string
  display: string
  options: Array<readonly [string, string]>
  onChange: (value: string) => void
}) {
  return (
    <Select value={value} onValueChange={(next) => next && onChange(next)}>
      <SelectTrigger
        size="sm"
        aria-label={label}
        className="max-w-40 border-transparent bg-muted/65 text-xs shadow-none hover:bg-muted dark:bg-muted/65"
      >
        {icon}
        <SelectValue>{display}</SelectValue>
      </SelectTrigger>
      <SelectContent size="compact">
        {options.map(([option, text]) => (
          <SelectItem key={option} value={option}>
            {text}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

function SearchableProperty({
  label,
  icon,
  value,
  display,
  placeholder,
  options,
  onChange,
}: {
  label: string
  icon: React.ReactNode
  value: string
  display: string
  placeholder: string
  options: SearchOption[]
  onChange: (value: string) => void
}) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState("")
  const matches = options.filter((option) => searchOptionMatches(option, query))

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        render={
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className="max-w-44 gap-1.5 bg-muted/65 text-xs font-normal"
            aria-label={label}
          />
        }
      >
        {icon}
        <span className={cn("truncate", !value && "text-muted-foreground")}>{display}</span>
      </PopoverTrigger>
      <PopoverContent className="w-72 gap-1 p-1.5" align="start" sideOffset={6}>
        <Input
          autoFocus
          className="h-8 text-xs"
          aria-label={placeholder}
          placeholder={placeholder}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <Separator className="my-1" />
        <div className="max-h-56 overflow-y-auto">
          {matches.map((option) => (
            <Button
              key={option.value || "__none__"}
              type="button"
              variant="ghost"
              size="sm"
              className="w-full justify-start font-normal"
              onClick={() => {
                onChange(option.value)
                setOpen(false)
                setQuery("")
              }}
            >
              <Check className={cn("mr-0.5", option.value !== value && "invisible")} />
              <span className="min-w-0 flex-1 truncate text-left">{option.label}</span>
              {option.detail && (
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
                  {option.detail}
                </span>
              )}
            </Button>
          ))}
          {!matches.length && (
            <p className="px-2 py-5 text-center text-xs text-muted-foreground">No matches.</p>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function LabelProperty({
  value,
  options,
  onChange,
}: {
  value: string[]
  options: string[]
  onChange: (value: string[]) => void
}) {
  const [query, setQuery] = useState("")
  const available = useMemo(() => labelOptions(options, value), [options, value])
  const items = labelItems(
    available.filter((label) =>
      label.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()),
    ),
    query,
  )
  const exact = available.some(
    (label) => label.toLocaleLowerCase() === query.trim().toLocaleLowerCase(),
  )
  const display =
    value.length === 0 ? "Labels" : value.length === 1 ? value[0] : `${value.length} labels`

  return (
    <Popover onOpenChange={(open) => !open && setQuery("")}>
      <PopoverTrigger
        render={
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className="max-w-40 gap-1.5 bg-muted/65 text-xs font-normal"
            aria-label="Labels"
          />
        }
      >
        <Tags />
        <span className={cn("truncate", !value.length && "text-muted-foreground")}>{display}</span>
      </PopoverTrigger>
      <PopoverContent className="w-72 gap-1 p-1.5" align="start" sideOffset={6}>
        <Input
          autoFocus
          className="h-8 text-xs"
          aria-label="Search or create labels"
          placeholder="Search or create labels…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <Separator className="my-1" />
        <div className="max-h-56 overflow-y-auto">
          {items.map((label) => {
            const selected = value.includes(label)
            const isCreate = Boolean(query.trim()) && label === query.trim() && !exact
            return (
              <Button
                key={label}
                type="button"
                variant="ghost"
                size="sm"
                className="w-full justify-start font-normal"
                onClick={() => {
                  onChange(selected ? value.filter((item) => item !== label) : [...value, label])
                  setQuery("")
                }}
              >
                {isCreate ? (
                  <CircleDot className="text-muted-foreground" />
                ) : (
                  <Check className={cn(!selected && "invisible")} />
                )}
                <span className="truncate">{isCreate ? `Create “${label}”` : label}</span>
              </Button>
            )
          })}
          {!items.length && (
            <p className="px-2 py-5 text-center text-xs text-muted-foreground">
              No labels in this workspace.
            </p>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function capitalize(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1)
}
