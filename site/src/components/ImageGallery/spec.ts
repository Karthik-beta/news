import { z } from "zod/v4";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { imageFields, imageMarkdown } from "../ImageFigure/spec";
import { ImageGallery } from "./ImageGallery";

export const GALLERY_MAX = 12;

export default defineNewsComponent({
  kind: "component",
  name: "ImageGallery",
  index: "ImageGallery — 2–12 informative images, opens full size — images[]",
  description:
    "A grid of informative images, each opening full size with its caption and credit; no carousel. Set crop only when the source allows square crops.",
  dataShape:
    "images: objects with the ImageFigure fields (src, alt, width, height, caption) from the brief's image records; no credit or licence (the renderer adds them).",
  example: `photos = ImageGallery([{src: "https://images.example.org/a.jpg", alt: "The lander on the surface.", width: 1600, height: 1200}, {src: "https://images.example.org/b.jpg", alt: "The descent stage.", width: 1600, height: 1200}], "Images returned in the first week.")`,
  props: z.object({
    images: z.array(z.object(imageFields)).min(2).max(GALLERY_MAX).describe("Two to twelve image records"),
    caption: z.string().nullable().optional().describe("What the images show together, one sentence"),
    crop: z.boolean().nullable().optional().describe("Square crops; only when the source allows"),
  }),
  component: ImageGallery,
  markdown: ({ props }) =>
    [props.caption?.trim() ? escapeMarkdown(props.caption.trim()) : "", ...props.images.map(imageMarkdown)]
      .filter(Boolean)
      .join("\n\n"),
});
