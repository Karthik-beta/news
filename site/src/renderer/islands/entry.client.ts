// The shared client entry for production pages: hydrates every island the generated registry knows.
// A page with no islands never loads this file (the renderer adds the <script> only when there is one).
import { islands } from "../../components/islands.generated";
import { hydrateIslands } from "./hydrate";

hydrateIslands(islands);
