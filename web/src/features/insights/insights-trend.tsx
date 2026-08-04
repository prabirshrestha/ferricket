import { SlidersHorizontal } from "lucide-react"
import { useId, useMemo, useRef, useState } from "react"
import { Button } from "../../components/ui/button"
import { Input } from "../../components/ui/input"
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
import { ToggleGroup, ToggleGroupItem } from "../../components/ui/toggle-group"
import { Tooltip, TooltipContent, TooltipTrigger } from "../../components/ui/tooltip"
import type { Ticket } from "../../types/ticket"
import {
  aggregateTrend,
  nearestTrendPointIndex,
  type TrendBin,
  type TrendRange,
  type TrendSeries,
} from "./insights-data"
import type { InsightsRangeState } from "./insights-range"

const seriesMeta: Record<TrendSeries, { label: string; color: string }> = {
  created: { label: "Created", color: "var(--viz-series-1)" },
  completed: { label: "Completed", color: "var(--viz-series-4)" },
  inProgress: { label: "In progress", color: "var(--viz-series-2)" },
  open: { label: "Open", color: "var(--viz-series-3)" },
}
const binOptions: Array<[TrendBin, string]> = [
  ["auto", "Auto"],
  ["1m", "1 minute"],
  ["30m", "30 minutes"],
  ["1d", "1 day"],
  ["custom", "Custom minutes"],
]

export function InsightsTrend({
  tickets,
  range,
  activityTimestamps,
  onRangeSelect,
}: {
  tickets: Ticket[]
  range: InsightsRangeState
  activityTimestamps: string[]
  onRangeSelect: (start: Date, end: Date) => void
}) {
  const [bin, setBin] = useState<TrendBin>("auto")
  const [customMinutes, setCustomMinutes] = useState(15)
  const [series, setSeries] = useState<TrendSeries[]>([
    "created",
    "completed",
    "inProgress",
    "open",
  ])
  const result = useMemo(
    () =>
      aggregateTrend(tickets, {
        ...range,
        bin,
        customMinutes,
        eventDates: activityTimestamps,
      }),
    [tickets, range, bin, customMinutes, activityTimestamps],
  )
  const selected = series.length ? series : (["open"] as TrendSeries[])

  return (
    <section className="overflow-hidden rounded-xl border bg-card">
      <header className="flex flex-wrap items-center gap-2 border-b px-4 py-3">
        <div className="mr-auto min-w-48">
          <h2 className="text-sm font-semibold">Ticket activity over time</h2>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {formatWindow(result.start, result.end)} · {formatBin(result.binMinutes)} bins
            {result.limited ? " (limited for readability)" : ""}
          </p>
        </div>
        <SeriesToggles value={series} onChange={setSeries} />
        <ChartSettings
          bin={bin}
          customMinutes={customMinutes}
          onBinChange={setBin}
          onMinutesChange={setCustomMinutes}
        />
      </header>
      <TrendChart result={result} series={selected} onRangeSelect={onRangeSelect} />
    </section>
  )
}

function SeriesToggles({
  value,
  onChange,
}: {
  value: TrendSeries[]
  onChange: (value: TrendSeries[]) => void
}) {
  const keys = Object.keys(seriesMeta) as TrendSeries[]
  return (
    <ToggleGroup
      aria-label="Visible trend series"
      value={value}
      onValueChange={(values) => {
        const next = values.filter((item): item is TrendSeries =>
          keys.includes(item as TrendSeries),
        )
        if (next.length) onChange(next)
      }}
      variant="outline"
      size="sm"
      spacing={0}
      className="max-w-full overflow-x-auto"
    >
      {keys.map((key) => {
        const active = value.includes(key)
        return (
          <Tooltip key={key}>
            <TooltipTrigger
              render={
                <ToggleGroupItem
                  value={key}
                  aria-label={`${active ? "Hide" : "Show"} ${seriesMeta[key].label}`}
                  className="gap-1.5 px-2.5 font-normal data-pressed:font-medium"
                />
              }
            >
              <span
                className="size-2 shrink-0 rounded-full"
                style={{ background: seriesMeta[key].color }}
              />
              <span>{seriesMeta[key].label}</span>
            </TooltipTrigger>
            <TooltipContent>
              {active ? "Hide" : "Show"} {seriesMeta[key].label}
            </TooltipContent>
          </Tooltip>
        )
      })}
    </ToggleGroup>
  )
}

function ChartSettings({
  bin,
  customMinutes,
  onBinChange,
  onMinutesChange,
}: {
  bin: TrendBin
  customMinutes: number
  onBinChange: (value: TrendBin) => void
  onMinutesChange: (value: number) => void
}) {
  return (
    <Popover>
      <PopoverTrigger
        render={<Button variant="ghost" size="icon-sm" aria-label="Chart settings" />}
      >
        <SlidersHorizontal />
      </PopoverTrigger>
      <PopoverContent className="w-72 gap-0 p-1.5" align="end" sideOffset={6}>
        <PopoverHeader className="px-2 py-1.5">
          <PopoverTitle className="text-xs">Chart settings</PopoverTitle>
        </PopoverHeader>
        <Separator className="my-1" />
        <div className="grid gap-2 p-2">
          <ChartSelect
            label="Bin size"
            value={bin}
            options={binOptions}
            onChange={(value) => onBinChange(value as TrendBin)}
          />
          {bin === "custom" && (
            <label className="grid grid-cols-[64px_minmax(0,1fr)] items-center gap-2 text-xs text-muted-foreground">
              <span>Minutes</span>
              <Input
                className="h-7 text-xs"
                type="number"
                min="1"
                max="43200"
                value={customMinutes}
                onChange={(event) => onMinutesChange(Number(event.target.value) || 1)}
              />
            </label>
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function TrendChart({
  result,
  series,
  onRangeSelect,
}: {
  result: ReturnType<typeof aggregateTrend>
  series: TrendSeries[]
  onRangeSelect: (start: Date, end: Date) => void
}) {
  const svg = useRef<SVGSVGElement>(null)
  const tooltipId = useId()
  const [selection, setSelection] = useState<{ start: number; current: number } | null>(null)
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null)
  const { points } = result
  const width = 900
  const height = 300
  const plot = { left: 48, right: 884, top: 24, bottom: 242 }
  const max = Math.max(1, ...points.flatMap((point) => series.map((key) => point[key])))
  const yTicks = [...new Set([0, 0.25, 0.5, 0.75, 1].map((fraction) => Math.round(max * fraction)))]
  const x = (index: number) =>
    plot.left + (index * (plot.right - plot.left)) / Math.max(1, points.length - 1)
  const y = (value: number) => plot.bottom - (value * (plot.bottom - plot.top)) / max
  const path = (key: TrendSeries) =>
    points.map((point, index) => `${index ? "L" : "M"}${x(index)},${y(point[key])}`).join(" ")
  const ticks = [...new Set([0, Math.floor((points.length - 1) / 2), points.length - 1])]
  const plotX = (clientX: number) => {
    const rect = svg.current?.getBoundingClientRect()
    if (!rect) return plot.left
    return Math.max(plot.left, Math.min(plot.right, ((clientX - rect.left) / rect.width) * width))
  }
  const inspectAt = (clientX: number) => {
    const position = plotX(clientX)
    setHoveredIndex(nearestTrendPointIndex(position, points.length, plot.left, plot.right))
    return position
  }
  const finishSelection = () => {
    if (!selection) return
    const left = Math.min(selection.start, selection.current)
    const right = Math.max(selection.start, selection.current)
    setSelection(null)
    if (right - left < 8) return
    const toTime = (value: number) =>
      result.start.getTime() +
      ((value - plot.left) / (plot.right - plot.left)) *
        (result.end.getTime() - result.start.getTime())
    onRangeSelect(new Date(toTime(left)), new Date(toTime(right)))
  }

  const hovered = hoveredIndex === null ? undefined : points[hoveredIndex]
  const hoveredX = hoveredIndex === null ? 0 : x(hoveredIndex)

  return (
    <div className="px-4 pt-4">
      <div className="relative">
        <svg
          ref={svg}
          className="h-auto w-full outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
          viewBox={`0 0 ${width} ${height}`}
          role="img"
          tabIndex={0}
          aria-label={`Ticket activity from ${formatWindow(result.start, result.end)}`}
          aria-describedby={hovered ? tooltipId : undefined}
          onFocus={() => setHoveredIndex((current) => current ?? Math.max(0, points.length - 1))}
          onBlur={() => setHoveredIndex(null)}
          onKeyDown={(event) => {
            if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return
            event.preventDefault()
            const direction = event.key === "ArrowRight" ? 1 : -1
            setHoveredIndex((current) =>
              Math.max(0, Math.min(points.length - 1, (current ?? points.length - 1) + direction)),
            )
          }}
          onPointerDown={(event) => {
            event.currentTarget.setPointerCapture(event.pointerId)
            const point = inspectAt(event.clientX)
            setSelection({ start: point, current: point })
          }}
          onPointerMove={(event) => {
            const point = inspectAt(event.clientX)
            setSelection((current) => (current ? { ...current, current: point } : current))
          }}
          onPointerUp={finishSelection}
          onPointerLeave={() => !selection && setHoveredIndex(null)}
          onPointerCancel={() => {
            setSelection(null)
            setHoveredIndex(null)
          }}
        >
          <desc>
            {series
              .map((key) => `${seriesMeta[key].label}: ${points.at(-1)?.[key] ?? 0}`)
              .join(", ")}
            . In-progress activity uses the latest file update when transition history is
            unavailable.
          </desc>
          {yTicks.map((value) => {
            const lineY = y(value)
            return (
              <g key={value}>
                <line x1={plot.left} x2={plot.right} y1={lineY} y2={lineY} stroke="var(--border)" />
                <text
                  x={plot.left - 10}
                  y={lineY + 4}
                  textAnchor="end"
                  fill="var(--muted-foreground)"
                  fontSize="11"
                >
                  {value}
                </text>
              </g>
            )
          })}
          {ticks.map((index) => (
            <text
              key={index}
              x={x(index)}
              y={plot.bottom + 24}
              textAnchor={index === 0 ? "start" : index === points.length - 1 ? "end" : "middle"}
              fill="var(--muted-foreground)"
              fontSize="11"
            >
              {formatTick(points[index]?.timestamp, result.binMinutes)}
            </text>
          ))}
          {series.map((key) => (
            <g key={key}>
              <path
                d={path(key)}
                fill="none"
                stroke={seriesMeta[key].color}
                strokeWidth="2"
                strokeLinejoin="round"
                strokeLinecap="round"
              />
              <circle
                cx={x(points.length - 1)}
                cy={y(points.at(-1)?.[key] ?? 0)}
                r="3"
                fill={seriesMeta[key].color}
              />
            </g>
          ))}
          {hovered && !selection && (
            <g pointerEvents="none">
              <line
                x1={hoveredX}
                x2={hoveredX}
                y1={plot.top}
                y2={plot.bottom}
                stroke="var(--muted-foreground)"
                strokeWidth="1"
                strokeDasharray="3 3"
                opacity="0.65"
              />
              {series.map((key) => (
                <circle
                  key={key}
                  cx={hoveredX}
                  cy={y(hovered[key])}
                  r="4"
                  fill="var(--card)"
                  stroke={seriesMeta[key].color}
                  strokeWidth="2.5"
                />
              ))}
            </g>
          )}
          {selection && (
            <rect
              x={Math.min(selection.start, selection.current)}
              y={plot.top}
              width={Math.abs(selection.current - selection.start)}
              height={plot.bottom - plot.top}
              fill="var(--primary)"
              fillOpacity="0.14"
              stroke="var(--primary)"
              strokeWidth="1"
              pointerEvents="none"
            />
          )}
        </svg>
        {hovered && !selection && (
          <div
            id={tooltipId}
            role="tooltip"
            className={
              "pointer-events-none absolute top-3 z-10 min-w-48 rounded-lg border bg-popover/95 p-2.5 text-popover-foreground shadow-lg backdrop-blur-sm " +
              (hoveredX > width * 0.72 ? "-translate-x-full -ml-2" : "ml-2")
            }
            style={{ left: `${(hoveredX / width) * 100}%` }}
          >
            <div className="mb-2 border-b pb-2">
              <p className="text-xs font-medium">
                {formatHoverWindow(hovered.timestamp, result.binMinutes, result.end)}
              </p>
              <p className="mt-0.5 text-[10px] text-muted-foreground">
                {formatBin(result.binMinutes)} bin
              </p>
            </div>
            <div className="grid gap-1.5">
              {series.map((key) => (
                <div className="flex items-center gap-2 text-xs" key={key}>
                  <span
                    className="size-2 shrink-0 rounded-full"
                    style={{ background: seriesMeta[key].color }}
                  />
                  <span className="text-muted-foreground">{seriesMeta[key].label}</span>
                  <span className="ml-auto font-medium tabular-nums">{hovered[key]}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
      <p className="-mt-3 pb-3 text-center text-[11px] text-muted-foreground">
        Hover for details · Drag to zoom all Insights charts · Focus and use ← → to inspect
      </p>
    </div>
  )
}

function ChartSelect({
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
  const selected = options.find(([option]) => option === value)?.[1] ?? value
  return (
    <label className="grid grid-cols-[64px_minmax(0,1fr)] items-center gap-2 text-xs text-muted-foreground">
      <span>{label}</span>
      <Select value={value} onValueChange={(next) => next && onChange(next)}>
        <SelectTrigger size="sm" className="w-full">
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

function formatWindow(start: Date, end: Date) {
  const format = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  })
  return `${format.format(start)} – ${format.format(end)}`
}
function formatTick(value: string | undefined, binMinutes: number) {
  if (!value) return ""
  return new Intl.DateTimeFormat(
    undefined,
    binMinutes < 24 * 60
      ? { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }
      : { month: "short", day: "numeric" },
  ).format(new Date(value))
}
function formatHoverWindow(value: string, binMinutes: number, rangeEnd: Date) {
  const start = new Date(value)
  const end = new Date(Math.min(rangeEnd.getTime(), start.getTime() + binMinutes * 60_000))
  const date = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" })
  const time = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" })
  if (binMinutes >= 24 * 60) return date.format(start)
  if (start.toDateString() === end.toDateString())
    return `${date.format(start)}, ${time.format(start)} – ${time.format(end)}`
  return `${date.format(start)}, ${time.format(start)} – ${date.format(end)}, ${time.format(end)}`
}
function formatBin(minutes: number) {
  if (minutes < 60) return `${minutes} min`
  if (minutes < 24 * 60) return `${Math.round(minutes / 60)} hr`
  return `${Math.round(minutes / (24 * 60))} day`
}
