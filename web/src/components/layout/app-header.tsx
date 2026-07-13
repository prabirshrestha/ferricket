import { BookmarkPlus, Pause, Play, Plus, Radio, Search } from "lucide-react"
import { Button } from "../ui/button"
import { viewLabel, type Ticket, type View } from "../../types/ticket"
import type { ResolvedTheme, Theme } from "../../hooks/use-theme"
import type { DisplayPreferences } from "../../hooks/use-display-preferences"
import type { FilterState } from "../../features/filters/filter-types"
import { FilterBuilder } from "../../features/filters/filter-builder"
import { DisplayMenu } from "./display-menu"
import { ThemeMenu } from "./theme-menu"
import { SidebarTrigger } from "../ui/sidebar"
import { LayoutSwitcher } from "./layout-switcher"
import { Tabs, TabsList, TabsTrigger } from "../ui/tabs"

type Props = {
  view: View
  tickets: Ticket[]
  filters: FilterState
  display: DisplayPreferences
  theme: Theme
  resolvedTheme: ResolvedTheme
  onFilters: (filters: FilterState) => void
  onDisplay: (patch: Partial<DisplayPreferences>) => void
  onSearch: () => void
  onCreate: () => void
  onThemeChange: (theme: Theme) => void
  onView: (view: View) => void
  live: boolean
  watchEnabled: boolean
  pendingChanges: number
  onLiveChange: (live: boolean) => void
  onSaveView: () => void
}

export function AppHeader({
  view,
  tickets,
  filters,
  display,
  theme,
  resolvedTheme,
  live,
  watchEnabled,
  pendingChanges,
  onLiveChange,
  onSaveView,
  onFilters,
  onDisplay,
  onSearch,
  onCreate,
  onThemeChange,
  onView,
}: Props) {
  return (
    <>
      <header className="sticky top-0 z-10 flex h-12 items-center border-b bg-background/90 px-3 backdrop-blur-xl sm:px-5">
        <SidebarTrigger
          className="mr-2"
          aria-label={display.sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
        />
        <Tabs value={view} onValueChange={(value) => onView(value as View)}>
          <TabsList variant="line" className="h-12 gap-0">
            {(["all", "active", "blocked"] as View[]).map((item) => (
              <TabsTrigger key={item} value={item} className="h-12 px-3 text-xs font-medium">
                {viewLabel[item]}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <div className="ml-auto flex items-center gap-1.5">
          <Button
            className="w-8 justify-start gap-2 text-muted-foreground sm:w-56"
            variant="outline"
            size="sm"
            onClick={onSearch}
          >
            <Search />
            <span className="hidden min-w-0 flex-1 truncate text-left font-normal sm:block">
              Search tickets…
            </span>
            <kbd className="ml-auto hidden rounded border bg-muted px-1.5 font-mono text-[10px] leading-4 text-muted-foreground sm:block">
              ⌘K
            </kbd>
          </Button>
          {watchEnabled ? (
            <Button
              variant="ghost"
              size="sm"
              className={
                (live ? "text-emerald-600 dark:text-emerald-400" : "text-muted-foreground") +
                " hidden md:inline-flex"
              }
              onClick={() => onLiveChange(!live)}
              title={live ? "Pause live updates" : "Resume live updates"}
            >
              {live ? <Radio /> : <Play />}
              <span>
                {live ? "Live" : pendingChanges ? `Paused · ${pendingChanges}` : "Paused"}
              </span>
            </Button>
          ) : (
            <span className="hidden items-center gap-1.5 text-xs text-muted-foreground md:flex">
              <Pause className="size-3.5" />
              Watch off
            </span>
          )}
          <ThemeMenu theme={theme} resolvedTheme={resolvedTheme} onThemeChange={onThemeChange} />
          <Button size="sm" onClick={onCreate}>
            <Plus /> <span className="max-[440px]:hidden">New ticket</span>
            <kbd className="rounded border border-primary-foreground/15 bg-black/15 px-1 font-mono text-[9px] leading-4 text-primary-foreground max-sm:hidden">
              C
            </kbd>
          </Button>
        </div>
      </header>
      <div className="sticky top-12 z-[9] flex h-10 items-center gap-1 border-b bg-background/95 px-3 backdrop-blur-xl sm:px-5">
        <FilterBuilder tickets={tickets} filters={filters} onChange={onFilters} />
        <div className="flex-1" />
        <Button variant="ghost" size="sm" onClick={onSaveView}>
          <BookmarkPlus /> <span className="hidden sm:inline">Save view</span>
        </Button>
        <LayoutSwitcher value={display.layout} onChange={(layout) => onDisplay({ layout })} />
        <DisplayMenu view={view} display={display} onChange={onDisplay} />
      </div>
    </>
  )
}
