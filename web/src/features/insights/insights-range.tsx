import { CalendarRange } from "lucide-react"
import { useState } from "react"
import { Button } from "../../components/ui/button"
import { Calendar } from "../../components/ui/calendar"
import {
  Popover,
  PopoverContent,
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
import { Separator } from "../../components/ui/separator"
import type { TrendRange } from "./insights-data"

export type InsightsRangeState = {
  range: TrendRange
  customStart: string
  customEnd: string
}

export const defaultInsightsRange: InsightsRangeState = {
  range: "smart",
  customStart: "",
  customEnd: "",
}

const options: Array<[TrendRange, string]> = [
  ["smart", "Smart"],
  ["1h", "1 hour"],
  ["24h", "24 hours"],
  ["7d", "7 days"],
  ["30d", "30 days"],
  ["90d", "90 days"],
  ["custom", "Custom"],
]

export function InsightsRange({
  value,
  onChange,
}: {
  value: InsightsRangeState
  onChange: (value: InsightsRangeState) => void
}) {
  const label = options.find(([range]) => range === value.range)?.[1] ?? value.range
  return (
    <Popover>
      <PopoverTrigger render={<Button variant="outline" size="sm" />}>
        <CalendarRange /> {label}
      </PopoverTrigger>
      <PopoverContent className="w-80 gap-0 p-1.5" align="end" sideOffset={6}>
        <PopoverHeader className="px-2 py-1.5">
          <PopoverTitle className="text-xs">Insights range</PopoverTitle>
        </PopoverHeader>
        <Separator className="my-1" />
        <div className="grid gap-2 p-2">
          <label className="grid grid-cols-[56px_minmax(0,1fr)] items-center gap-2 text-xs text-muted-foreground">
            <span>Range</span>
            <Select
              value={value.range}
              onValueChange={(range) => range && onChange({ ...value, range: range as TrendRange })}
            >
              <SelectTrigger size="sm" className="w-full">
                <SelectValue>{label}</SelectValue>
              </SelectTrigger>
              <SelectContent size="compact">
                {options.map(([range, text]) => (
                  <SelectItem key={range} value={range}>
                    {text}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </label>
          {value.range === "custom" && (
            <>
              <DateTimePicker
                label="Start"
                value={value.customStart}
                onChange={(customStart) => onChange({ ...value, customStart })}
              />
              <DateTimePicker
                label="End"
                value={value.customEnd}
                onChange={(customEnd) => onChange({ ...value, customEnd })}
              />
            </>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function DateTimePicker({
  label,
  value,
  onChange,
}: {
  label: string
  value: string
  onChange: (value: string) => void
}) {
  const parsed = parseLocalDateTime(value)
  const [open, setOpen] = useState(false)
  const time = value.includes("T") ? value.split("T")[1]?.slice(0, 5) || "00:00" : "00:00"
  const [hour, minute] = time.split(":")
  const update = (date: Date, nextTime = time) => onChange(toLocalDateTime(date, nextTime))
  return (
    <div className="grid grid-cols-[56px_minmax(0,1fr)] items-center gap-2 text-xs text-muted-foreground">
      <span>{label}</span>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          render={<Button variant="outline" size="sm" className="justify-start font-normal" />}
        >
          <CalendarRange />
          {parsed ? formatDateTime(parsed) : `Choose ${label.toLowerCase()}`}
        </PopoverTrigger>
        <PopoverContent className="w-auto gap-0 p-0" align="start" side="left" sideOffset={12}>
          <Calendar
            mode="single"
            selected={parsed ?? undefined}
            defaultMonth={parsed ?? undefined}
            onSelect={(date) => date && update(date)}
          />
          <Separator />
          <div className="flex items-center gap-2 p-2">
            <span className="text-xs text-muted-foreground">Time</span>
            <div className="ml-auto flex items-center gap-1">
              <TimeSelect
                label="Hour"
                value={hour ?? "00"}
                values={Array.from({ length: 24 }, (_, item) => String(item).padStart(2, "0"))}
                onChange={(next) => update(parsed ?? new Date(), `${next}:${minute ?? "00"}`)}
              />
              <span className="text-muted-foreground">:</span>
              <TimeSelect
                label="Minute"
                value={minute ?? "00"}
                values={Array.from({ length: 60 }, (_, item) => String(item).padStart(2, "0"))}
                onChange={(next) => update(parsed ?? new Date(), `${hour ?? "00"}:${next}`)}
              />
            </div>
            <Button size="sm" onClick={() => setOpen(false)}>
              Done
            </Button>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  )
}

function TimeSelect({
  label,
  value,
  values,
  onChange,
}: {
  label: string
  value: string
  values: string[]
  onChange: (value: string) => void
}) {
  return (
    <Select value={value} onValueChange={(next) => next && onChange(next)}>
      <SelectTrigger size="sm" className="w-16" aria-label={label}>
        <SelectValue>{value}</SelectValue>
      </SelectTrigger>
      <SelectContent size="compact" className="w-20 min-w-20">
        {values.map((item) => (
          <SelectItem key={item} value={item}>
            {item}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}

export function toLocalDateTime(date: Date, time?: string) {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  const resolvedTime =
    time ??
    `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`
  return `${year}-${month}-${day}T${resolvedTime}`
}

function parseLocalDateTime(value: string) {
  if (!value) return null
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? null : date
}

function formatDateTime(value: Date) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(value)
}
