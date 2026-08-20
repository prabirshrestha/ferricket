import { useState } from "react"
import {
  Archive,
  Ban,
  BarChart3,
  Bookmark,
  Check,
  Copy,
  Folder,
  Inbox,
  LayoutList,
  X,
} from "lucide-react"
import type { Meta, Ticket, View } from "../../types/ticket"
import type { SavedView } from "../../hooks/use-workspace-preferences"
import { Button } from "../ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "../ui/tooltip"
import {
  Sidebar as SidebarPrimitive,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  SidebarSeparator,
  useSidebar,
} from "../ui/sidebar"

type Props = {
  meta: Meta
  tickets: Ticket[]
  view: View
  onView: (view: View) => void
  savedViews: SavedView[]
  activeSavedViewId?: string
  insightsActive?: boolean
  onInsights: () => void
  onSavedView: (view: SavedView) => void
  onRemoveSavedView: (id: string) => void
}

export function Sidebar({
  meta,
  tickets,
  view,
  onView,
  savedViews,
  activeSavedViewId,
  insightsActive,
  onInsights,
  onSavedView,
  onRemoveSavedView,
}: Props) {
  const { setOpenMobile } = useSidebar()
  const selectView = (next: View) => {
    onView(next)
    setOpenMobile(false)
  }
  const workspaceName = pathName(meta.workspace || meta.tickets_dir) || "Workspace"
  const ticketsLocation = relativePath(meta.workspace, meta.tickets_dir)
  return (
    <SidebarPrimitive collapsible="icon" className="border-r bg-muted/60">
      <SidebarHeader className="p-2">
        <div className="flex h-10 items-center gap-2 px-1">
          <div className="grid size-6 shrink-0 place-items-center rounded-md bg-gradient-to-br from-indigo-400 to-indigo-700 text-xs font-semibold text-white shadow-inner">
            F
          </div>
          <div className="flex min-w-0 flex-1 flex-col overflow-hidden group-data-[collapsible=icon]:hidden">
            <span className="w-full truncate text-sm font-semibold tracking-[-.012em]">
              {workspaceName}
            </span>
            <span className="text-[10px] text-muted-foreground">Ferricket</span>
          </div>
        </div>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup className="pt-0">
          <SidebarGroupContent>
            <SidebarMenu>
              <NavItem
                icon={Inbox}
                label="Active"
                active={!insightsActive && view === "active"}
                count={tickets.filter((t) => t.status !== "closed").length}
                onClick={() => selectView("active")}
              />
              <NavItem
                icon={LayoutList}
                label="All tickets"
                active={!insightsActive && view === "all"}
                count={tickets.length}
                onClick={() => selectView("all")}
              />
              <NavItem
                icon={Ban}
                label="Blocked"
                active={!insightsActive && view === "blocked"}
                count={tickets.filter((t) => t.blocked && t.status !== "closed").length}
                onClick={() => selectView("blocked")}
              />
              <NavItem
                icon={Archive}
                label="Closed"
                active={!insightsActive && view === "closed"}
                onClick={() => selectView("closed")}
              />
              <NavItem
                icon={BarChart3}
                label="Insights"
                active={insightsActive}
                onClick={() => {
                  onInsights()
                  setOpenMobile(false)
                }}
              />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
        {savedViews.length > 0 && (
          <SidebarGroup>
            <SidebarGroupLabel>Views</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {savedViews.map((saved) => (
                  <SidebarMenuItem key={saved.id}>
                    <SidebarMenuButton
                      tooltip={saved.name}
                      isActive={activeSavedViewId === saved.id}
                      onClick={() => {
                        onSavedView(saved)
                        setOpenMobile(false)
                      }}
                    >
                      <Bookmark />
                      <span>{saved.name}</span>
                    </SidebarMenuButton>
                    <Button
                      variant="ghost"
                      size="icon-xs"
                      className="absolute top-1.5 right-2 hidden text-muted-foreground group-data-[collapsible=icon]:hidden group-hover/menu-item:flex"
                      aria-label={"Remove " + saved.name}
                      onClick={(event) => {
                        event.stopPropagation()
                        onRemoveSavedView(saved.id)
                      }}
                    >
                      <X />
                    </Button>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        )}
      </SidebarContent>
      <SidebarFooter className="gap-1 text-[10px] text-muted-foreground group-data-[collapsible=icon]:hidden">
        <SidebarSeparator className="mx-0" />
        <PathButton
          label={workspaceName}
          path={meta.workspace || meta.tickets_dir}
          icon={<Folder />}
        />
        <PathButton label={ticketsLocation} path={meta.tickets_dir} />
        <span className="ml-6 font-mono">fer {meta.version}</span>
      </SidebarFooter>
      <SidebarRail />
    </SidebarPrimitive>
  )
}

function PathButton({
  label,
  path,
  icon,
}: {
  label: string
  path: string
  icon?: React.ReactNode
}) {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(path)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1200)
    } catch {
      setCopied(false)
    }
  }
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="xs"
            className="group/path h-6 w-full justify-start gap-1.5 px-1 font-mono font-normal text-muted-foreground"
            onClick={() => void copy()}
          />
        }
      >
        <span className="grid size-4 shrink-0 place-items-center [&_svg]:size-3.5">{icon}</span>
        <span className="min-w-0 flex-1 truncate text-left">{label}</span>
        {copied ? (
          <Check className="text-emerald-500" />
        ) : (
          <Copy className="opacity-0 transition-opacity group-hover/path:opacity-70" />
        )}
      </TooltipTrigger>
      <TooltipContent side="right" align="end">
        <span className="font-mono">{path}</span>
        <span>{copied ? "Copied" : "Click to copy"}</span>
      </TooltipContent>
    </Tooltip>
  )
}

function pathName(path: string) {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? ""
}
function relativePath(workspace: string, ticketsDir: string) {
  const root = workspace.replace(/[\\/]+$/, "")
  return root && ticketsDir.startsWith(root)
    ? ticketsDir.slice(root.length).replace(/^[\\/]+/, "") || ".tickets"
    : pathName(ticketsDir)
}

function NavItem({
  icon: Icon,
  label,
  active,
  count,
  onClick,
}: {
  icon: typeof Inbox
  label: string
  active?: boolean
  count?: number
  onClick: () => void
}) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        className="text-xs font-normal"
        isActive={active}
        tooltip={label}
        onClick={onClick}
      >
        <Icon />
        <span>{label}</span>
      </SidebarMenuButton>
      {count !== undefined && <SidebarMenuBadge>{count}</SidebarMenuBadge>}
    </SidebarMenuItem>
  )
}
