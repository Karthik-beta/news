// A gallery of images that each carry information (design/components.md, "Gallery"): 2 columns on phones, 3 from
// tablet width, 8px gaps; square crops only when the source allows. Each opens full size in an overlay with its
// caption and credit. The grid and overlay are an island (ImageGallery.client.tsx); with JavaScript off every
// image is still a link to the full-size file. Every credit is shown under the grid without opening anything.
import type { ComponentRenderer } from "@openuidev/react-lang";
import { useId } from "react";
import { Island } from "../../renderer/islands/Island";
import { creditLine, type ImageData } from "../ImageFigure/ImageFigure";
import ImageGalleryIsland from "./ImageGallery.client";

export interface ImageGalleryProps {
  images: ImageData[];
  caption?: string | null;
  crop?: boolean | null;
}

/** The distinct credit lines, in the order the images use them. */
export const galleryCredits = (images: readonly ImageData[]) => [...new Set(images.map(creditLine))];

/** Only the fields the island needs, as plain JSON (no undefined values). */
const plain = (image: ImageData): ImageData => ({
  src: image.src,
  alt: image.alt,
  width: image.width,
  height: image.height,
  credit: image.credit,
  licence: image.licence,
  caption: image.caption ?? null,
});

export const ImageGallery: ComponentRenderer<ImageGalleryProps> = ({ props }) => {
  const captionId = `gallery-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const caption = props.caption?.trim() || null;
  return (
    <figure className="image-gallery" aria-labelledby={caption ? captionId : undefined}>
      <Island
        name="ImageGallery"
        component={ImageGalleryIsland}
        props={{ images: props.images.map(plain), crop: props.crop === true, labelledBy: caption ? captionId : null }}
      />
      <figcaption className="image-gallery__caption">
        {caption ? (
          <span className="text-label" id={captionId}>
            {caption}
          </span>
        ) : null}
        {galleryCredits(props.images).map((line) => (
          <span key={line} className="text-source">
            {line}
          </span>
        ))}
      </figcaption>
    </figure>
  );
};
