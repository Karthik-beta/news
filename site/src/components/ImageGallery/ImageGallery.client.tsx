// The gallery's interactive part (an island): a grid of images, each opening full size in a simple overlay with
// its caption and credit. No carousel, no autoplay, no swiping between images.
//
// Server and client render the same markup: each image is a link to the full-size file, so with JavaScript off
// (or before hydration) it still opens. Once hydrated, a click opens the overlay instead: a native modal <dialog>,
// so focus moves into it, Escape closes it, the page behind is inert, and focus returns to the image that opened it.
import { useEffect, useId, useRef, useState } from "react";
import { Icon } from "../Icon/Icon";
import { creditLine, type ImageData, ImageFrame } from "../ImageFigure/ImageFigure";

export interface ImageGalleryIslandProps {
  images: ImageData[];
  /** Square crops, only when the source allows cropping. */
  crop: boolean;
  /** The id of the gallery's caption, which names the list of images. */
  labelledBy: string | null;
}

export default function ImageGalleryIsland({ images, crop, labelledBy }: ImageGalleryIslandProps) {
  const [open, setOpen] = useState<number | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const triggers = useRef<(HTMLAnchorElement | null)[]>([]);
  const lastOpened = useRef<number | null>(null);

  useEffect(() => {
    const el = dialog.current;
    if (!el) return;
    if (open !== null && !el.open) el.showModal();
    if (open === null && el.open) el.close();
  }, [open]);

  const onClose = () => {
    setOpen(null);
    const i = lastOpened.current;
    if (i !== null) triggers.current[i]?.focus();
  };

  const image = open === null ? null : images[open];
  const titleId = useId();

  return (
    <>
      <ul
        className={`image-gallery__grid${crop ? " image-gallery__grid--crop" : ""}`}
        aria-labelledby={labelledBy ?? undefined}
      >
        {images.map((img, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: a static list in a fixed order; the same file may appear twice
          <li key={i} className="image-gallery__item">
            <a
              className="image-gallery__open"
              href={img.src}
              ref={(el) => {
                triggers.current[i] = el;
              }}
              onClick={(event) => {
                if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) return;
                event.preventDefault();
                lastOpened.current = i;
                setOpen(i);
              }}
            >
              <ImageFrame image={img} className={crop ? "image-frame--square" : undefined} />
            </a>
          </li>
        ))}
      </ul>
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: the click is on the backdrop; Escape and the close button are the keyboard routes */}
      <dialog
        ref={dialog}
        className="image-gallery__overlay"
        aria-labelledby={image ? titleId : undefined}
        onClose={onClose}
        onClick={(event) => {
          if (event.target === event.currentTarget) setOpen(null);
        }}
      >
        {image ? (
          <figure className="image-gallery__full">
            <button
              type="button"
              className="image-gallery__close text-ui"
              onClick={() => setOpen(null)}
              // biome-ignore lint/a11y/noAutofocus: the modal's first stop is its close button
              autoFocus
            >
              <Icon name="close" /> Close
            </button>
            <ImageFrame image={image} loading="eager" className="image-gallery__full-frame" />
            <figcaption className="image-gallery__full-caption">
              <span className="text-label" id={titleId}>
                {image.caption?.trim() || image.alt}
              </span>
              <span className="text-source">{creditLine(image)}</span>
            </figcaption>
          </figure>
        ) : null}
      </dialog>
    </>
  );
}
