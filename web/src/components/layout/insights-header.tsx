import { BarChart3, Plus, Search } from "lucide-react"
import type { ResolvedTheme, Theme } from "../../hooks/use-theme"
import { Button } from "../ui/button"
import { SidebarTrigger } from "../ui/sidebar"
import { ThemeMenu } from "./theme-menu"

export function InsightsHeader({
  theme,
  resolvedTheme,
  onSearch,
  onCreate,
  onThemeChange,
}: {
  theme: Theme
  resolvedTheme: ResolvedTheme
  onSearch: () => void
  onCreate: () => void
  onThemeChange: (theme: Theme) => void
}) {
  return (
    <header className="sticky top-0 z-10 flex h-12 items-center border-b bg-background/90 px-3 backdrop-blur-xl sm:px-5">
      <SidebarTrigger className="mr-2" />
      <div className="flex items-center gap-2 text-[13px]">
        <BarChart3 className="size-4 text-muted-foreground" />
        <strong className="font-medium">Insights</strong>
      </div>
      <div className="ml-auto flex items-center gap-2">
        <Button
          className="w-9 justify-start gap-2 text-muted-foreground sm:w-56"
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
        <ThemeMenu theme={theme} resolvedTheme={resolvedTheme} onThemeChange={onThemeChange} />
        <Button size="sm" onClick={onCreate}>
          <Plus /> <span className="max-[440px]:hidden">New ticket</span>
        </Button>
      </div>
    </header>
  )
}
