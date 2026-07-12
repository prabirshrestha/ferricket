import { BarChart3, RotateCcw } from "lucide-react"
import { useMemo, useState } from "react"
import { Button } from "../../components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../../components/ui/select"
import type { Ticket } from "../../types/ticket"
import {
  aggregateInsights,
  resolveTrendWindow,
  ticketsTouchedInWindow,
  type InsightRow,
  type InsightSlice,
} from "./insights-data"
import {
  defaultInsightsRange,
  InsightsRange,
  toLocalDateTime,
  type InsightsRangeState,
} from "./insights-range"
import { InsightsTrend } from "./insights-trend"

export { aggregateInsights, aggregateTrend } from "./insights-data"

const slices: Array<[InsightSlice, string]> = [
  ["status", "Status"],
  ["priority", "Priority"],
  ["type", "Type"],
  ["assignee", "Assignee"],
  ["labels", "Labels"],
]

const colors = [
  "var(--viz-series-1)",
  "var(--viz-series-2)",
  "var(--viz-series-3)",
  "var(--viz-series-4)",
  "var(--viz-series-5)",
]

export function InsightsView({ tickets }: { tickets: Ticket[] }) {
  const [slice, setSlice] = useState<InsightSlice>("status")
  const [range, setRange] = useState<InsightsRangeState>(defaultInsightsRange)
  const [rangeBeforeZoom, setRangeBeforeZoom] = useState<InsightsRangeState | null>(null)
  const window = useMemo(
    () => resolveTrendWindow(tickets, { ...range, bin: "auto" }),
    [tickets, range],
  )
  const windowTickets = useMemo(
    () => ticketsTouchedInWindow(tickets, window.start, window.end),
    [tickets, window],
  )
  const rows = useMemo(() => aggregateInsights(windowTickets, slice), [windowTickets, slice])
  const updateRange = (next: InsightsRangeState) => {
    setRangeBeforeZoom(null)
    setRange(next)
  }
  const selectChartRange = (start: Date, end: Date) => {
    setRangeBeforeZoom((current) => current ?? range)
    setRange({
      range: "custom",
      customStart: toLocalDateTime(start),
      customEnd: toLocalDateTime(end),
    })
  }

  return (
    <section className="mx-auto max-w-5xl px-4 py-6 pb-20 sm:px-7">
      <div className="mb-5 flex flex-wrap items-center gap-3">
        <div className="mr-auto">
          <h1 className="text-xl font-semibold tracking-[-.03em]">Insights</h1>
          <p className="mt-1 text-xs text-muted-foreground">
            {windowTickets.length} {windowTickets.length === 1 ? "ticket" : "tickets"} touched ·{" "}
            {formatWindow(window.start, window.end)}
          </p>
        </div>
        <div className="flex items-center gap-1.5" aria-label="Shared Insights range">
          {rangeBeforeZoom && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setRange(rangeBeforeZoom)
                setRangeBeforeZoom(null)
              }}
            >
              <RotateCcw /> Reset zoom
            </Button>
          )}
          <InsightsRange value={range} onChange={updateRange} />
        </div>
      </div>
      <div className="grid gap-5">
        <InsightsTrend tickets={tickets} range={range} onRangeSelect={selectChartRange} />
        <BreakdownPanel
          tickets={windowTickets}
          rows={rows}
          slice={slice}
          rangeStart={window.start}
          rangeEnd={window.end}
          onSliceChange={setSlice}
        />
      </div>
    </section>
  )
}

function BreakdownPanel({
  tickets,
  rows,
  slice,
  rangeStart,
  rangeEnd,
  onSliceChange,
}: {
  tickets: Ticket[]
  rows: InsightRow[]
  slice: InsightSlice
  rangeStart: Date
  rangeEnd: Date
  onSliceChange: (slice: InsightSlice) => void
}) {
  const chartRows = rows.slice(0, 12)
  const max = Math.max(1, ...chartRows.map((row) => row.value))
  const width = 900
  const plotBottom = 214
  const plotHeight = 198
  const gap = 10
  const barWidth = Math.max(
    12,
    (width - 64 - gap * Math.max(0, chartRows.length - 1)) / Math.max(1, chartRows.length),
  )

  return (
    <section className="overflow-hidden rounded-xl border bg-card">
      <header className="flex flex-wrap items-end gap-3 border-b px-4 py-3">
        <div className="mr-auto">
          <div className="text-2xl font-medium tracking-tight">
            {tickets.length}
            <span className="ml-1.5 text-sm font-normal text-muted-foreground">
              {tickets.length === 1 ? "ticket" : "tickets"}
            </span>
          </div>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {formatWindow(rangeStart, rangeEnd)} · shared Insights range
          </p>
        </div>
        <Control
          label="Slice"
          value={slice}
          options={slices}
          onChange={(value) => onSliceChange(value as InsightSlice)}
        />
      </header>
      <div className="px-4 pt-4">
        {chartRows.length ? (
          <svg
            className="h-auto w-full"
            viewBox="0 0 900 260"
            role="img"
            aria-label={`Ticket count by ${slice}`}
          >
            <title>Ticket count by {slice}</title>
            <desc>{chartRows.map((row) => `${row.label}: ${row.value}`).join(", ")}</desc>
            {[0, 0.25, 0.5, 0.75, 1].map((fraction) => {
              const lineY = plotBottom - plotHeight * fraction
              return (
                <g key={fraction}>
                  <line x1="44" x2="892" y1={lineY} y2={lineY} stroke="var(--border)" />
                  <text
                    x="36"
                    y={lineY + 4}
                    textAnchor="end"
                    fill="var(--muted-foreground)"
                    fontSize="11"
                  >
                    {Math.round(max * fraction)}
                  </text>
                </g>
              )
            })}
            {chartRows.map((row, index) => {
              const x = 52 + index * (barWidth + gap)
              const barHeight = Math.max(2, (plotHeight * row.value) / max)
              return (
                <g key={row.key}>
                  <rect
                    x={x}
                    y={plotBottom - barHeight}
                    width={barWidth}
                    height={barHeight}
                    rx="2"
                    fill={colors[index % colors.length]}
                  >
                    <title>
                      {row.label}: {row.value}
                    </title>
                  </rect>
                  <text
                    x={x + barWidth / 2}
                    y={plotBottom + 18}
                    textAnchor="middle"
                    fill="var(--muted-foreground)"
                    fontSize="11"
                  >
                    {truncate(row.label, 12)}
                  </text>
                </g>
              )
            })}
          </svg>
        ) : (
          <div className="grid h-64 place-items-center text-sm text-muted-foreground">
            <div className="grid justify-items-center gap-2">
              <BarChart3 className="size-6" />
              <span>No ticket data</span>
            </div>
          </div>
        )}
      </div>
      <div className="border-t">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b bg-muted/40 text-left text-xs text-muted-foreground">
              <th className="px-4 py-2.5 font-medium">
                {slices.find((item) => item[0] === slice)?.[1]}
              </th>
              <th className="w-36 px-4 py-2.5 font-medium">Ticket count</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr className="border-b last:border-b-0" key={row.key}>
                <td className="px-4 py-2.5">{row.label}</td>
                <td className="px-4 py-2.5 tabular-nums">{row.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}

function Control({
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
    <label className="grid gap-1 text-xs text-muted-foreground">
      <span>{label}</span>
      <Select value={value} onValueChange={(next) => next && onChange(next)}>
        <SelectTrigger className="w-36">
          <SelectValue>{selected}</SelectValue>
        </SelectTrigger>
        <SelectContent size="compact">
          {options.map(([option, text]) => (
            <SelectItem key={option} value={option}>
              {text}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </label>
  )
}

function truncate(value: string, size: number) {
  return value.length > size ? value.slice(0, size - 1) + "…" : value
}

function formatWindow(start: Date, end: Date) {
  const format = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  })
  return `${format.format(start)} – ${format.format(end)}`
}
