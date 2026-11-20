import { z } from "zod/v4";
import { escapeMarkdown } from "../../renderer/markdown";
import { defineNewsComponent } from "../contract";
import { creditLine, type ImageData, ImageFigure } from "./ImageFigure";

/** An https URL or a path on this site. Never javascript:, data: or protocol-relative. */
export const IMAGE_SRC = /^(https:\/\/[^\s"'<>]+|\/(?!\/)[^\s"'<>]*)$/;

export const imageFields = {
  src: z.string().regex(IMAGE_SRC).describe("Image URL from the image record (https, or a path on this site)"),
  alt: z.string().min(1).describe("What it shows and why, one sentence; not the credit"),
  width: z.number().int().positive().describe("Width in pixels, from the image record"),
  height: z.number().int().positive().describe("Height in pixels, from the image record"),
  credit: z.string().min(1).describe("Credit as the source requires ('NASA/JPL-Caltech')"),
  licence: z.string().min(1).describe("'Public domain', 'CC BY 4.0'"),
  caption: z.string().nullable().optional().describe("One sentence"),
};

/** Markdown for one image: the picture, then the caption and credit as its own paragraph. */
export function imageMarkdown(image: ImageData): string {
  const caption = [image.caption?.trim(), creditLine(image)].filter(Boolean).join(" ");
  return `![${escapeMarkdown(image.alt)}](${image.src})\n\n${escapeMarkdown(caption)}`;
}

export default defineNewsComponent({
  kind: "component",
  name: "ImageFigure",
  index: "ImageFigure — informative image, credited — image record fields",
  description:
    "An image that carries information (never decoration), with alt text, a one-sentence caption, and the credit and licence from the image record.",
  dataShape:
    "Copy src, width, height, credit and licence from the brief's image record; alt says what it shows; caption is one sentence.",
  example: `photo = ImageFigure("https://images.example.org/pia26000.jpg", "The spacecraft on the launch pad before lift-off.", 1600, 900, "NASA/JPL-Caltech", "Public domain", "The probe was moved to the pad two days before launch.")`,
  props: z.object(imageFields),
  component: ImageFigure,
  markdown: ({ props }) => imageMarkdown(props),
});
