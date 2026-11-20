// An image that carries information, with its caption, credit and licence (design/components.md, "Image with
// credit"). Full column width, or the image's own width when smaller; no border, radius or shadow.
//
// The frame reserves the image's aspect ratio before it loads, so the layout never shifts. If the image fails to
// load (blocked, offline, gone), the frame shows the alt text and the credit, centred on the wash, at the same size.
// No JavaScript, two layers from the same data attributes: a broken image draws its own ::after over itself
// (Chromium, Firefox; a loaded image never draws it), and the frame's ::before sits behind the image, where it
// shows through a broken image in every engine, WebKit included, which draws no ::after on one. Static: 0 KB JS.
import type { ComponentRenderer } from "@openuidev/react-lang";
import type { CSSProperties } from "react";

export interface ImageData {
  /** https URL, or a path on this site. */
  src: string;
  /** What the image shows and why it is there, in one sentence. Not the credit. */
  alt: string;
  /** Intrinsic size in pixels, from the image record. Sets the aspect ratio before the image loads. */
  width: number;
  height: number;
  /** Who made it ("NASA/JPL-Caltech"). */
  credit: string;
  /** "Public domain", "CC BY 4.0". */
  licence: string;
  /** One sentence. */
  caption?: string | null;
}

/** "NASA/JPL-Caltech. Public domain." */
export const creditLine = (image: Pick<ImageData, "credit" | "licence">) =>
  [image.credit, image.licence]
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => (/[.!?]$/.test(s) ? s : `${s}.`))
    .join(" ");

/**
 * The frame's computed geometry: the image's intrinsic size as two custom properties, from which the stylesheet
 * takes the aspect ratio and the natural width the frame grows to at most.
 */
export const frameStyle = (image: Pick<ImageData, "width" | "height">): CSSProperties =>
  ({ "--image-w": String(image.width), "--image-h": String(image.height) }) as CSSProperties;

export interface ImageFrameProps {
  image: ImageData;
  /** "lazy" below the fold (the default); "eager" for an image near the top. */
  loading?: "lazy" | "eager";
  className?: string;
}

/** The image in its fixed-ratio frame, with the failure fallback. Shared with the gallery. */
export function ImageFrame({ image, loading = "lazy", className }: ImageFrameProps) {
  return (
    <span
      className={`image-frame${className ? ` ${className}` : ""}`}
      style={frameStyle(image)}
      data-alt={image.alt}
      data-credit={creditLine(image)}
    >
      <img
        className="image-frame__img"
        src={image.src}
        alt={image.alt}
        width={image.width}
        height={image.height}
        loading={loading}
        decoding="async"
        data-credit={creditLine(image)}
      />
    </span>
  );
}

export const ImageFigure: ComponentRenderer<ImageData> = ({ props }) => (
  <figure className="image-figure">
    <ImageFrame image={props} />
    <figcaption className="image-figure__caption">
      {props.caption ? <span className="image-figure__text text-label">{props.caption}</span> : null}
      <span className="image-figure__credit text-source">{creditLine(props)}</span>
    </figcaption>
  </figure>
);
