import * as React from "react"
import { Autocomplete as CommandPrimitive } from "@base-ui/react/autocomplete"
import { Search } from "lucide-react"

import { cn } from "@/lib/utils"

export type CommandItemValue = { id: string; label: string; keywords?: string; detail?: string }

function Command(
  props: Omit<CommandPrimitive.Root.Props<CommandItemValue>, "items"> & {
    items?: readonly CommandItemValue[]
  },
) {
  return (
    <CommandPrimitive.Root
      autoHighlight="always"
      inline
      open
      itemToStringValue={(item) => item.label}
      mode="list"
      {...props}
    />
  )
}

function CommandInput({ className, ...props }: CommandPrimitive.Input.Props) {
  return (
    <div className="flex h-12 items-center gap-3 border-b px-4">
      <Search className="size-4 shrink-0 text-muted-foreground" />
      <CommandPrimitive.Input
        className={cn(
          "min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground",
          className,
        )}
        {...props}
      />
    </div>
  )
}

function CommandList({ className, ...props }: CommandPrimitive.List.Props) {
  return (
    <CommandPrimitive.List
      className={cn(
        "max-h-[min(60vh,460px)] overflow-y-auto overscroll-contain p-2 outline-none",
        className,
      )}
      {...props}
    />
  )
}

function CommandEmpty({ className, ...props }: CommandPrimitive.Empty.Props) {
  return (
    <CommandPrimitive.Empty
      className={cn("px-3 py-10 text-center text-sm text-muted-foreground", className)}
      {...props}
    />
  )
}

function CommandItem({ className, ...props }: CommandPrimitive.Item.Props) {
  return (
    <CommandPrimitive.Item
      className={cn(
        "flex w-full cursor-default items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm outline-none data-highlighted:bg-accent data-highlighted:text-accent-foreground",
        className,
      )}
      {...props}
    />
  )
}

function CommandGroup({ className, ...props }: CommandPrimitive.Group.Props) {
  return <CommandPrimitive.Group className={cn("overflow-hidden", className)} {...props} />
}

function CommandGroupLabel({ className, ...props }: CommandPrimitive.GroupLabel.Props) {
  return (
    <CommandPrimitive.GroupLabel
      className={cn(
        "px-3 py-2 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground",
        className,
      )}
      {...props}
    />
  )
}

function CommandSeparator({ className, ...props }: CommandPrimitive.Separator.Props) {
  return <CommandPrimitive.Separator className={cn("my-1 h-px bg-border", className)} {...props} />
}

function CommandShortcut({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      className={cn("ml-auto text-xs tracking-widest text-muted-foreground", className)}
      {...props}
    />
  )
}

export {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandGroupLabel,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
}
