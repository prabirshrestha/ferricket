import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { X } from "lucide-react"
import { Button } from "./components/ui/button"
import { AppHeader } from "./components/layout/app-header"
import { Sidebar } from "./components/layout/sidebar"
import { CreateTicketDialog } from "./features/tickets/create-ticket-dialog"
import { EmptyState } from "./features/tickets/empty-state"
import { TicketDetailView } from "./features/tickets/ticket-detail-view"
import { TicketGroup } from "./features/tickets/ticket-group"
import { useTickets } from "./hooks/use-tickets"
import { useTheme } from "./hooks/use-theme"
import { useDisplayPreferences, type Density } from "./hooks/use-display-preferences"
import { SidebarProvider } from "./components/ui/sidebar"
import { ticketMatchesFilters } from "./features/filters/filter-types"
import { emptyFilters } from "./features/filters/filter-types"
import { TicketHierarchy, withAncestorContext } from "./features/tickets/ticket-hierarchy"
import { CommandPalette } from "./components/layout/command-palette"
import { isInput } from "./lib/format"
import { viewLabel, type Status, type Ticket, type View } from "./types/ticket"
import { useWorkspacePreferences } from "./hooks/use-workspace-preferences"
import { BulkActionBar } from "./features/tickets/bulk-action-bar"
import { SaveViewDialog } from "./components/layout/save-view-dialog"
import { TicketBoard } from "./features/tickets/ticket-board"
import { groupTickets, orderTickets, ticketsForView } from "./features/tickets/display-policy"
import { parseAppRoute, savedViewRoute, ticketRoute, viewRoute } from "./lib/routes"
import { insightsRoute } from "./lib/routes"
import { InsightsView } from "./features/insights/insights-view"
import { InsightsHeader } from "./components/layout/insights-header"
import { TicketDndProvider } from "./features/tickets/ticket-dnd"
import { DependencyRoadmapView } from "./features/tickets/dependency-roadmap-view"
import { withDependencyContext } from "./features/tickets/dependency-roadmap"
import { DependencyDagView } from "./features/tickets/dependency-dag-view"
import { RawTicketView } from "./features/tickets/raw-ticket-view"
import { rawTicketRoute } from "./lib/routes"
import { useBrowserLocation } from "./hooks/use-browser-location"

const groups: Status[] = ["in_progress", "open", "closed"]

export function App() {
  const {
    tickets,
    meta,
    error,
    setError,
    refresh,
    update,
    add,
    live,
    setLive,
    resumeLive,
    pendingChanges,
    setTickets,
  } = useTickets()
  const { theme, resolvedTheme, setTheme } = useTheme()
  const { hydrated: displayHydrated, display, updateDisplay } = useDisplayPreferences()
  const {
    hydrated: workspaceHydrated,
    view,
    filters,
    savedViews,
    setView,
    setFilters,
    saveView,
    removeView,
    applyView,
  } = useWorkspacePreferences()
  const { pathname, navigate } = useBrowserLocation()
  const route = useMemo(() => parseAppRoute(pathname), [pathname])
  const selected = route.ticketId ?? null
  const appliedSavedRoute = useRef<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [createParent, setCreateParent] = useState<Ticket | undefined>()
  const [createStatus, setCreateStatus] = useState<Status>("open")
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [savingView, setSavingView] = useState(false)
  const [canvasFocused, setCanvasFocused] = useState(false)
  const lastSelected = useRef<string | null>(null)
  const restoreFocusTo = useRef<string | null>(null)

  useEffect(() => {
    if (
      pathname === "/" ||
      (!route.view && !route.savedViewId && !route.ticketId && !route.insights)
    )
      navigate(viewRoute("active"), { replace: true })
  }, [navigate, pathname, route])
  useEffect(() => {
    if (route.view && route.view !== view) setView(route.view)
  }, [route.view, setView, view])
  useEffect(() => {
    if (!route.savedViewId) {
      appliedSavedRoute.current = null
      return
    }
    if (!workspaceHydrated || !displayHydrated) return
    if (appliedSavedRoute.current === route.savedViewId) return
    const saved = savedViews.find((item) => item.id === route.savedViewId)
    if (!saved) {
      if (workspaceHydrated) navigate(viewRoute("active"), { replace: true })
      return
    }
    applyView(saved)
    updateDisplay(saved.display)
    appliedSavedRoute.current = saved.id
  }, [
    applyView,
    displayHydrated,
    navigate,
    route.savedViewId,
    savedViews,
    updateDisplay,
    workspaceHydrated,
  ])

  const visible = useMemo(
    () =>
      ticketsForView(tickets, view, display.showClosed).filter((ticket) =>
        ticketMatchesFilters(ticket, tickets, filters, meta.current_user),
      ),
    [tickets, view, display.showClosed, filters, meta.current_user],
  )
  const routeBase = route.savedViewId
    ? savedViewRoute(route.savedViewId)
    : viewRoute(route.view ?? view)
  const closeTicket = useCallback(() => {
    const id = selected ?? restoreFocusTo.current
    navigate(routeBase)
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (id) document.querySelector<HTMLElement>(`[data-ticket-id="${CSS.escape(id)}"]`)?.focus()
      })
    })
  }, [navigate, routeBase, selected])
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (
        event.key.toLowerCase() === "c" &&
        !event.metaKey &&
        !event.ctrlKey &&
        !isInput(event.target) &&
        !selected &&
        !paletteOpen
      ) {
        event.preventDefault()
        event.stopPropagation()
        setCreating(true)
        requestAnimationFrame(() =>
          (document.querySelector("#create-title") as HTMLInputElement | null)?.select(),
        )
      }
      if (event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault()
        setPaletteOpen(true)
      }
      if (event.key === "Escape" && !creating && !paletteOpen) {
        if (document.querySelector("[data-canvas-finder-open]")) return
        if (canvasFocused) {
          event.preventDefault()
          event.stopPropagation()
          setCanvasFocused(false)
          return
        }
        if (selected) closeTicket()
        setSelectedIds(new Set())
      }
      if (
        event.key.toLowerCase() === "a" &&
        (event.metaKey || event.ctrlKey) &&
        !isInput(event.target) &&
        !selected &&
        !creating &&
        !paletteOpen
      ) {
        event.preventDefault()
        setSelectedIds(new Set(visible.map((ticket) => ticket.id)))
      }
    }
    window.addEventListener("keydown", keydown, true)
    return () => window.removeEventListener("keydown", keydown, true)
  }, [visible, selected, creating, paletteOpen, closeTicket, canvasFocused])
  useEffect(() => {
    if (selected || route.insights || (display.layout !== "roadmap" && display.layout !== "dag"))
      setCanvasFocused(false)
  }, [display.layout, route.insights, selected])
  const ordered = useMemo(() => orderTickets(visible, display), [visible, display])
  const hierarchyTickets = useMemo(
    () =>
      filters.query?.trim() || filters.conditions.length || filters.groups?.length
        ? withAncestorContext(ordered, tickets)
        : ordered,
    [ordered, tickets, filters],
  )
  const roadmapTickets = useMemo(
    () =>
      display.layout === "roadmap" || display.layout === "dag"
        ? withDependencyContext(ordered, tickets)
        : ordered,
    [display.layout, ordered, tickets],
  )
  const roadmapContextIds = useMemo(() => {
    if (display.layout !== "roadmap" && display.layout !== "dag") return new Set<string>()
    const matched = new Set(ordered.map((ticket) => ticket.id))
    return new Set(
      roadmapTickets.filter((ticket) => !matched.has(ticket.id)).map((ticket) => ticket.id),
    )
  }, [display.layout, ordered, roadmapTickets])
  const grouped = useMemo(() => groupTickets(ordered, display.groupBy), [ordered, display.groupBy])
  const current = tickets.find((ticket) => ticket.id === selected)
  const selectView = (next: View) => {
    setView(next)
    navigate(viewRoute(next))
  }
  const selectSavedView = (saved: (typeof savedViews)[number]) => {
    applyView(saved)
    updateDisplay(saved.display)
    appliedSavedRoute.current = saved.id
    navigate(savedViewRoute(saved.id))
  }
  const selectTicket = useCallback(
    (id: string) => {
      restoreFocusTo.current = id
      navigate(ticketRoute(routeBase, id))
    },
    [navigate, routeBase],
  )
  const toggleSelection = useCallback(
    (id: string, range = false) => {
      setSelectedIds((current) => {
        const next = new Set(current)
        if (range && lastSelected.current) {
          const ids = ordered.map((ticket) => ticket.id)
          const [start, end] = [ids.indexOf(lastSelected.current), ids.indexOf(id)].sort(
            (a, b) => a - b,
          )
          if (start >= 0) ids.slice(start, end + 1).forEach((value) => next.add(value))
        } else if (next.has(id)) next.delete(id)
        else next.add(id)
        return next
      })
      lastSelected.current = id
    },
    [ordered],
  )
  const selectedTickets = tickets.filter((ticket) => selectedIds.has(ticket.id))

  return (
    <SidebarProvider
      className="h-dvh min-h-full bg-background text-foreground"
      open={!display.sidebarCollapsed}
      onOpenChange={(open) => updateDisplay({ sidebarCollapsed: !open })}
      style={
        { "--sidebar-width": "14.5rem", "--sidebar-width-icon": "3.5rem" } as React.CSSProperties
      }
    >
      <Sidebar
        meta={meta}
        tickets={tickets}
        view={view}
        savedViews={savedViews}
        activeSavedViewId={route.savedViewId}
        insightsActive={route.insights}
        onInsights={() => navigate(insightsRoute())}
        onSavedView={selectSavedView}
        onRemoveSavedView={(id) => {
          removeView(id)
          if (route.savedViewId === id) navigate(viewRoute("active"))
        }}
        onView={selectView}
      />
      <main className="min-w-0 flex-1 overflow-hidden bg-background">
        {current && route.rawTicket ? (
          <RawTicketView
            ticket={current}
            onTicket={() => navigate(ticketRoute(routeBase, current.id))}
            onClose={closeTicket}
            error={error}
            onError={setError}
          />
        ) : current ? (
          <TicketDetailView
            ticket={current}
            all={tickets}
            currentUser={meta.current_user}
            error={error}
            onBack={() => navigate(-1)}
            onForward={() => navigate(1)}
            onSelect={selectTicket}
            onClose={closeTicket}
            onUpdate={update}
            onRefresh={refresh}
            onError={setError}
            onRaw={() => navigate(rawTicketRoute(routeBase, current.id))}
            onCreateSubTicket={(parent) => {
              setCreateParent(parent)
              setCreating(true)
            }}
          />
        ) : (
          <div className="h-full overflow-auto">
            {route.insights ? (
              <InsightsHeader
                theme={theme}
                resolvedTheme={resolvedTheme}
                onSearch={() => setPaletteOpen(true)}
                onCreate={() => setCreating(true)}
                onThemeChange={setTheme}
              />
            ) : (
              <AppHeader
                view={view}
                tickets={tickets}
                filters={filters}
                display={display}
                theme={theme}
                resolvedTheme={resolvedTheme}
                live={live}
                watchEnabled={meta.watch_enabled}
                pendingChanges={pendingChanges}
                onLiveChange={(value) => (value ? resumeLive() : setLive(false))}
                onSaveView={() => setSavingView(true)}
                onFilters={setFilters}
                onDisplay={updateDisplay}
                onSearch={() => setPaletteOpen(true)}
                onCreate={() => setCreating(true)}
                onThemeChange={setTheme}
                onView={selectView}
              />
            )}
            {error && (
              <div className="mx-6 mt-3 flex items-center justify-between rounded-lg border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
                {error}
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label="Dismiss error"
                  onClick={() => setError("")}
                >
                  <X />
                </Button>
              </div>
            )}
            {route.insights ? (
              <InsightsView tickets={tickets} />
            ) : (
              <TicketDndProvider tickets={tickets} onUpdate={update}>
                <section
                  className={
                    display.layout === "board" ||
                    display.layout === "roadmap" ||
                    display.layout === "dag"
                      ? "h-[calc(100dvh-5.5rem)] overflow-hidden"
                      : "pb-20"
                  }
                >
                  {visible.length === 0 ? (
                    <div className="px-4 py-8 sm:px-7">
                      <EmptyState
                        onCreate={() => setCreating(true)}
                        filtered={Boolean(
                          filters.query?.trim() ||
                          filters.conditions.length ||
                          filters.groups?.length,
                        )}
                        onClearFilters={() => setFilters(emptyFilters)}
                      />
                    </div>
                  ) : display.layout === "hierarchy" ? (
                    <div className="px-4 py-4 sm:px-6">
                      <TicketHierarchy
                        tickets={hierarchyTickets}
                        density={display.density}
                        selectedIds={selectedIds}
                        onToggle={toggleSelection}
                        onSelect={selectTicket}
                        onUpdate={update}
                      />
                    </div>
                  ) : display.layout === "board" ? (
                    <TicketBoard
                      tickets={ordered}
                      density={display.density}
                      selectedIds={selectedIds}
                      onToggle={toggleSelection}
                      onSelect={selectTicket}
                      onUpdate={update}
                      onCreate={(status) => {
                        setCreateStatus(status)
                        setCreating(true)
                      }}
                    />
                  ) : display.layout === "roadmap" ? (
                    <DependencyRoadmapView
                      tickets={roadmapTickets}
                      contextIds={roadmapContextIds}
                      density={display.density}
                      selectedIds={selectedIds}
                      canvasFocused={canvasFocused}
                      onCanvasFocusedChange={setCanvasFocused}
                      onToggle={toggleSelection}
                      onSelect={selectTicket}
                    />
                  ) : display.layout === "dag" ? (
                    <DependencyDagView
                      tickets={roadmapTickets}
                      contextIds={roadmapContextIds}
                      selectedIds={selectedIds}
                      resolvedTheme={resolvedTheme}
                      direction={display.dagDirection}
                      onDirectionChange={(dagDirection) => updateDisplay({ dagDirection })}
                      canvasFocused={canvasFocused}
                      onCanvasFocusedChange={setCanvasFocused}
                      onToggle={toggleSelection}
                      onSelect={selectTicket}
                    />
                  ) : (
                    <TicketGroups
                      groups={grouped}
                      groupByStatus={display.groupBy === "status"}
                      density={display.density}
                      selectedIds={selectedIds}
                      onToggle={toggleSelection}
                      onSelect={selectTicket}
                      onUpdate={update}
                      onCreate={() => setCreating(true)}
                    />
                  )}
                </section>
              </TicketDndProvider>
            )}
          </div>
        )}
      </main>
      <CommandPalette
        open={paletteOpen}
        tickets={tickets}
        onOpenChange={setPaletteOpen}
        onTicket={selectTicket}
        onCreate={() => setCreating(true)}
        onView={selectView}
        onToggleSidebar={() => updateDisplay({ sidebarCollapsed: !display.sidebarCollapsed })}
        onSaveView={() => setSavingView(true)}
        onToggleLive={() => (live ? setLive(false) : resumeLive())}
      />
      <CreateTicketDialog
        open={creating}
        parent={createParent}
        initialStatus={createStatus}
        tickets={tickets}
        currentUser={meta.current_user}
        onOpenChange={(open) => {
          setCreating(open)
          if (!open) {
            setCreateParent(undefined)
            setCreateStatus("open")
          }
        }}
        onCreated={(ticket) => {
          add(ticket)
          selectTicket(ticket.id)
        }}
      />
      {selectedTickets.length > 0 && (
        <BulkActionBar
          tickets={selectedTickets}
          onClear={() => setSelectedIds(new Set())}
          onResult={(updated, conflicts) => {
            setTickets((current) =>
              current.map((ticket) => updated.find((item) => item.id === ticket.id) ?? ticket),
            )
            setSelectedIds(new Set(conflicts))
            if (conflicts.length)
              setError(
                `${updated.length} tickets updated; ${conflicts.length} changed on disk and were skipped.`,
              )
          }}
        />
      )}
      <SaveViewDialog
        open={savingView}
        onOpenChange={setSavingView}
        onSave={(name) => saveView(name, display)}
      />
    </SidebarProvider>
  )
}

type TicketGroupsProps = {
  groups: Array<{ label: string; tickets: Ticket[] }>
  groupByStatus: boolean
  density: Density
  selectedIds: Set<string>
  onToggle: (id: string, range: boolean) => void
  onSelect: (id: string) => void
  onUpdate: (id: string, body: Partial<Ticket>) => Promise<void>
  onCreate: () => void
}

function TicketGroups({
  groups: ticketGroups,
  groupByStatus,
  density,
  selectedIds,
  onToggle,
  onSelect,
  onUpdate,
  onCreate,
}: TicketGroupsProps) {
  return (
    <div className="pt-2">
      {ticketGroups.map((group) => {
        const status = groupByStatus
          ? (group.label as Status)
          : (group.tickets[0]?.status ?? groups[0])
        return (
          <TicketGroup
            key={group.label}
            status={status}
            dropStatus={groupByStatus ? status : undefined}
            label={groupByStatus ? undefined : group.label}
            tickets={group.tickets}
            density={density}
            selectedIds={selectedIds}
            onToggle={onToggle}
            onSelect={onSelect}
            onUpdate={onUpdate}
            onCreate={onCreate}
          />
        )
      })}
    </div>
  )
}
