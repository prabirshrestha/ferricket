import { useState } from "react"
import { Button } from "../ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../ui/dialog"
import { Input } from "../ui/input"

export function SaveViewDialog({
  open,
  onOpenChange,
  onSave,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSave: (name: string) => void
}) {
  const [name, setName] = useState("")
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Save custom view</DialogTitle>
          <DialogDescription>
            Save the current view, nested filters, grouping, ordering, and density on this device.
          </DialogDescription>
        </DialogHeader>
        <Input
          autoFocus
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="View name"
          onKeyDown={(event) => {
            if (event.key === "Enter" && name.trim()) {
              onSave(name.trim())
              setName("")
              onOpenChange(false)
            }
          }}
        />
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={!name.trim()}
            onClick={() => {
              onSave(name.trim())
              setName("")
              onOpenChange(false)
            }}
          >
            Save view
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
