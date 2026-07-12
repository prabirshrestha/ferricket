import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { App } from "./App"
import { TooltipProvider } from "./components/ui/tooltip"
import "./styles.css"
import "@xyflow/react/dist/style.css"
import "@fontsource-variable/inter"
import "@fontsource-variable/geist-mono"

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider>
      <App />
    </TooltipProvider>
  </StrictMode>,
)
