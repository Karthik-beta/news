// Not used by any page. Bundled only by measure-react.ts to size the alternative the spike rejects:
// hydrating the whole OpenUI page (parser, runtime, every component) instead of only the chart island.
import { Renderer } from "@openuidev/react-lang";
import { hydrateRoot } from "react-dom/client";
import { spikeLibrary } from "./library";

const root = document.querySelector("main");
if (root) hydrateRoot(root, <Renderer response={root.dataset.oui ?? ""} library={spikeLibrary} />);
