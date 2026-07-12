import { Filter, Plus, Trash2, X } from "lucide-react"
import { useMemo, useRef, useState } from "react"
import { Badge } from "../../components/ui/badge"
import { Button } from "../../components/ui/button"
import { Input } from "../../components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from "../../components/ui/popover"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../components/ui/select"
import { ToggleGroup, ToggleGroupItem } from "../../components/ui/toggle-group"
import type { Ticket } from "../../types/ticket"
import {
  defaultCondition,
  fieldLabels,
  operatorsFor,
  type FilterCondition,
  type FilterField,
  type FilterState,
} from "./filter-types"
import { SearchCombobox } from "../tickets/search-combobox"
import {
  Autocomplete,
  AutocompleteContent,
  AutocompleteEmpty,
  AutocompleteInput,
  AutocompleteItem,
  AutocompleteList,
} from "../../components/ui/autocomplete"
import { applySuggestion, querySuggestions, type QuerySuggestion } from "./query-suggestions"

type Props = { tickets: Ticket[]; filters: FilterState; onChange: (filters: FilterState) => void }

const fields = Object.keys(fieldLabels) as FilterField[]

export function FilterBuilder({ tickets, filters, onChange }: Props) {
  const [popoverOpen, setPopoverOpen] = useState(false)
  const update = (id: string, patch: Partial<FilterCondition>) =>
    onChange({
      ...filters,
      conditions: filters.conditions.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    })
  const remove = (id: string) =>
    onChange({ ...filters, conditions: filters.conditions.filter((item) => item.id !== id) })
  const addGroup = () =>
    onChange({
      ...filters,
      groups: [
        ...(filters.groups ?? []),
        { id: crypto.randomUUID(), match: "all", conditions: [defaultCondition()], groups: [] },
      ],
    })
  const updateGroup = (id: string, group: FilterState) =>
    onChange({
      ...filters,
      groups: (filters.groups ?? []).map((item) => (item.id === id ? group : item)),
    })
  const removeGroup = (id: string) =>
    onChange({ ...filters, groups: (filters.groups ?? []).filter((item) => item.id !== id) })
  const total =
    filters.conditions.length +
    (filters.groups ?? []).reduce((count, group) => count + group.conditions.length, 0)
  const hasFilters = total > 0 || Boolean(filters.query?.trim())
  return (
    <div className="flex min-w-0 items-center gap-2">
      <Popover open={popoverOpen} onOpenChange={setPopoverOpen}>
        <PopoverTrigger render={<Button variant={hasFilters ? "secondary" : "ghost"} size="sm" />}>
          <Filter /> Filter{" "}
          {hasFilters && (
            <Badge className="ml-0.5 min-w-5 px-1.5">
              {total + (filters.query?.trim() ? 1 : 0)}
            </Badge>
          )}
        </PopoverTrigger>
        <PopoverContent className="w-[min(94vw,680px)] gap-3 p-3" align="start" sideOffset={7}>
          <PopoverHeader>
            <PopoverTitle>Filter tickets</PopoverTitle>
            <PopoverDescription>
              Type a Gmail-style query or combine visual conditions. Both apply together.
            </PopoverDescription>
          </PopoverHeader>
          <QueryInput
            query={filters.query ?? ""}
            tickets={tickets}
            onChange={(query) => onChange({ ...filters, query })}
            onCommit={() => setPopoverOpen(false)}
          />
          <div className="flex items-center gap-2">
            <span className="h-px flex-1 bg-border" />
            <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
              Visual conditions
            </span>
            <span className="h-px flex-1 bg-border" />
          </div>
          <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
            <span>Match</span>
            <ToggleGroup
              value={[filters.match]}
              onValueChange={(values) => {
                const value = values.at(-1)
                if (value === "all" || value === "any") onChange({ ...filters, match: value })
              }}
              variant="outline"
              size="sm"
              spacing={0}
            >
              <ToggleGroupItem value="all">all</ToggleGroupItem>
              <ToggleGroupItem value="any">any</ToggleGroupItem>
            </ToggleGroup>
            <span>of the following</span>
          </div>
          <div className="grid gap-2">
            {filters.conditions.map((condition) => (
              <ConditionRow
                key={condition.id}
                condition={condition}
                tickets={tickets}
                onChange={(patch) => update(condition.id, patch)}
                onRemove={() => remove(condition.id)}
              />
            ))}
          </div>
          {(filters.groups ?? []).map((group) => (
            <NestedGroup
              key={group.id}
              group={group}
              tickets={tickets}
              onChange={(next) => updateGroup(group.id!, next)}
              onRemove={() => removeGroup(group.id!)}
            />
          ))}
          {total === 0 && (
            <div className="rounded-lg border border-dashed px-3 py-6 text-center text-xs text-muted-foreground">
              {filters.query?.trim()
                ? "No visual conditions. The query above still applies."
                : "No conditions. Every ticket is shown."}
            </div>
          )}
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                onChange({ ...filters, conditions: [...filters.conditions, defaultCondition()] })
              }
            >
              <Plus /> Add condition
            </Button>
            <Button variant="ghost" size="sm" onClick={addGroup}>
              <Plus /> Add group
            </Button>
            {hasFilters && (
              <Button
                className="ml-auto"
                variant="ghost"
                size="sm"
                onClick={() => onChange({ match: "all", conditions: [], groups: [], query: "" })}
              >
                Clear all
              </Button>
            )}
          </div>
        </PopoverContent>
      </Popover>
      <div className="hidden min-w-0 items-center gap-1.5 md:flex">
        {filters.query?.trim() && (
          <Badge
            className="h-7 max-w-64 gap-1.5 rounded-lg bg-background px-2.5 font-mono text-[11px] font-normal"
            variant="outline"
          >
            <span className="truncate">{filters.query}</span>
            <Button
              variant="ghost"
              size="icon-xs"
              className="size-4"
              aria-label="Clear query filter"
              onClick={() => onChange({ ...filters, query: "" })}
            >
              <X />
            </Button>
          </Badge>
        )}
        {filters.conditions.slice(0, filters.query?.trim() ? 1 : 2).map((condition) => (
          <Badge
            className="h-7 max-w-48 gap-1.5 rounded-lg bg-background px-2.5 text-xs font-normal"
            variant="outline"
            key={condition.id}
          >
            <span className="truncate">
              <span className="text-muted-foreground">{fieldLabels[condition.field]}</span>{" "}
              {condition.value}
            </span>
            <Button
              variant="ghost"
              size="icon-xs"
              className="size-4"
              aria-label={"Remove " + fieldLabels[condition.field] + " filter"}
              onClick={() => remove(condition.id)}
            >
              <X />
            </Button>
          </Badge>
        ))}
      </div>
    </div>
  )
}

function QueryInput({
  query,
  tickets,
  onChange,
  onCommit,
}: {
  query: string
  tickets: Ticket[]
  onChange: (query: string) => void
  onCommit: () => void
}) {
  const input = useRef<HTMLInputElement>(null)
  const [cursor, setCursor] = useState(query.length)
  const [open, setOpen] = useState(false)
  const [highlighted, setHighlighted] = useState<QuerySuggestion | undefined>()
  const suggestions = useMemo(
    () => querySuggestions(query, tickets, cursor),
    [query, tickets, cursor],
  )
  const choose = (suggestion: QuerySuggestion) => {
    const next = applySuggestion(query, suggestion, cursor)
    const continueAutocomplete = !suggestion.complete
    onChange(next.value)
    setCursor(next.cursor)
    setOpen(continueAutocomplete)
    setHighlighted(undefined)
    requestAnimationFrame(() => {
      input.current?.focus()
      input.current?.setSelectionRange(next.cursor, next.cursor)
      if (continueAutocomplete) setOpen(true)
    })
  }
  return (
    <div className="grid gap-1.5">
      <Autocomplete
        items={suggestions}
        value={query}
        mode="none"
        open={open && suggestions.length > 0}
        onOpenChange={setOpen}
        onItemHighlighted={setHighlighted}
        onValueChange={(value, details) => {
          if (details.reason === "input-change" || details.reason === "input-clear") {
            const nextCursor = input.current?.selectionStart ?? value.length
            onChange(value)
            setCursor(nextCursor)
            setOpen(querySuggestions(value, tickets, nextCursor).length > 0)
          }
        }}
        autoHighlight="always"
        itemToStringValue={(item) => item.label}
      >
        <AutocompleteInput
          ref={input}
          aria-label="Filter query"
          className="h-9 font-mono text-xs"
          placeholder="status:open label:backend -assignee:me OR is:blocked"
          onFocus={(event) => {
            setCursor(event.currentTarget.selectionStart ?? query.length)
            setOpen(true)
          }}
          onClick={(event) => {
            setCursor(event.currentTarget.selectionStart ?? query.length)
            setOpen(true)
          }}
          onKeyUp={(event) => setCursor(event.currentTarget.selectionStart ?? query.length)}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === "Tab") {
              if (suggestions.length > 0) {
                event.preventDefault()
                choose(highlighted ?? suggestions[0])
              } else if (event.key === "Enter") {
                event.preventDefault()
                onCommit()
              }
            }
          }}
        />
        <AutocompleteContent className="min-w-[min(94vw,28rem)]">
          <AutocompleteList>
            {suggestions.map((suggestion) => (
              <AutocompleteItem
                key={suggestion.id}
                value={suggestion}
                onClick={() => choose(suggestion)}
              >
                <span className="min-w-0 flex-1 truncate font-mono text-xs">
                  {suggestion.label}
                </span>
                <span className="shrink-0 text-[10px] text-muted-foreground">
                  {suggestion.detail}
                </span>
              </AutocompleteItem>
            ))}
          </AutocompleteList>
          <AutocompleteEmpty>Keep typing a free-form query.</AutocompleteEmpty>
        </AutocompleteContent>
      </Autocomplete>
      <p className="text-[11px] text-muted-foreground">
        Use quotes, <code>OR</code>, <code>-negation</code>, or brace groups.
      </p>
    </div>
  )
}

function NestedGroup({
  group,
  tickets,
  onChange,
  onRemove,
}: {
  group: FilterState
  tickets: Ticket[]
  onChange: (group: FilterState) => void
  onRemove: () => void
}) {
  const update = (id: string, patch: Partial<FilterCondition>) =>
    onChange({
      ...group,
      conditions: group.conditions.map((item) => (item.id === id ? { ...item, ...patch } : item)),
    })
  return (
    <div className="ml-4 grid gap-2 rounded-lg border border-dashed bg-muted/30 p-2.5">
      <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
        <span>Nested group · match</span>
        <ToggleGroup
          value={[group.match]}
          onValueChange={(values) => {
            const value = values.at(-1)
            if (value === "all" || value === "any") onChange({ ...group, match: value })
          }}
          variant="outline"
          size="sm"
          spacing={0}
        >
          <ToggleGroupItem value="all">all</ToggleGroupItem>
          <ToggleGroupItem value="any">any</ToggleGroupItem>
        </ToggleGroup>
        <Button
          className="ml-auto"
          variant="ghost"
          size="icon-sm"
          aria-label="Remove group"
          onClick={onRemove}
        >
          <Trash2 />
        </Button>
      </div>
      {group.conditions.map((condition) => (
        <ConditionRow
          key={condition.id}
          condition={condition}
          tickets={tickets}
          onChange={(patch) => update(condition.id, patch)}
          onRemove={() =>
            onChange({
              ...group,
              conditions: group.conditions.filter((item) => item.id !== condition.id),
            })
          }
        />
      ))}
      <Button
        className="w-fit"
        variant="ghost"
        size="sm"
        onClick={() =>
          onChange({ ...group, conditions: [...group.conditions, defaultCondition()] })
        }
      >
        <Plus /> Add condition
      </Button>
    </div>
  )
}

function ConditionRow({
  condition,
  tickets,
  onChange,
  onRemove,
}: {
  condition: FilterCondition
  tickets: Ticket[]
  onChange: (patch: Partial<FilterCondition>) => void
  onRemove: () => void
}) {
  const values = valuesFor(condition.field, tickets)
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,.75fr)_minmax(0,1fr)_28px] gap-1.5">
      <Select
        value={condition.field}
        onValueChange={(value) => {
          const field = value as FilterField
          const next = defaultCondition(field)
          onChange({ field, operator: next.operator, value: next.value })
        }}
      >
        <SelectTrigger className="w-full" size="sm">
          <SelectValue>{fieldLabels[condition.field]}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          {fields.map((field) => (
            <SelectItem key={field} value={field}>
              {fieldLabels[field]}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Select
        value={condition.operator}
        onValueChange={(value) => onChange({ operator: value as FilterCondition["operator"] })}
      >
        <SelectTrigger className="w-full" size="sm">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {operatorsFor(condition.field).map((operator) => (
            <SelectItem key={operator.value} value={operator.value}>
              {operator.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {condition.field === "created" || (condition.field === "labels" && values.length === 0) ? (
        <Input
          className="h-7 text-[11px]"
          type={condition.field === "created" ? "date" : "text"}
          value={condition.value}
          placeholder="Value"
          onChange={(event) => onChange({ value: event.target.value })}
        />
      ) : condition.field === "labels" || condition.field === "assignee" ? (
        <SearchCombobox
          value={condition.value}
          options={values}
          placeholder={`Search ${fieldLabels[condition.field].toLowerCase()}…`}
          onChange={(value) => onChange({ value })}
        />
      ) : (
        <Select
          value={condition.value}
          onValueChange={(value) => value !== null && onChange({ value })}
        >
          <SelectTrigger className="w-full" size="sm">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {values.map((value) => (
              <SelectItem key={value.value} value={value.value}>
                {value.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
      <Button variant="ghost" size="icon-sm" aria-label="Remove condition" onClick={onRemove}>
        <Trash2 />
      </Button>
    </div>
  )
}

function valuesFor(field: FilterField, tickets: Ticket[]) {
  if (field === "status")
    return [
      { value: "open", label: "Todo" },
      { value: "in_progress", label: "In progress" },
      { value: "closed", label: "Done" },
    ]
  if (field === "priority")
    return [0, 1, 2, 3, 4].map((value) => ({ value: String(value), label: `P${value}` }))
  if (field === "blocked")
    return [
      { value: "true", label: "Blocked" },
      { value: "false", label: "Not blocked" },
    ]
  if (field === "relation")
    return [
      { value: "top_level", label: "Top-level" },
      { value: "sub_ticket", label: "Sub-ticket" },
      { value: "parent", label: "Has sub-tickets" },
    ]
  if (field === "assignee")
    return [
      { value: "unassigned", label: "Unassigned" },
      ...unique(tickets.flatMap((ticket) => (ticket.assignee ? [ticket.assignee] : []))).map(
        (value) => ({ value, label: value }),
      ),
    ]
  if (field === "labels")
    return unique(tickets.flatMap((ticket) => ticket.tags)).map((value) => ({
      value,
      label: value,
    }))
  if (field === "type")
    return unique(tickets.map((ticket) => ticket.type || "task")).map((value) => ({
      value,
      label: value,
    }))
  return []
}

function unique(values: string[]) {
  return [...new Set(values)].sort((a, b) => a.localeCompare(b))
}
