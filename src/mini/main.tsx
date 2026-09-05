import { createRoot } from "react-dom/client"
import "../index.css"
import { MiniApp } from "./MiniWindow"

const rootEl = document.getElementById("mini-root")
if (rootEl) {
  createRoot(rootEl).render(<MiniApp />)
}
