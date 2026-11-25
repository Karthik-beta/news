// The home page's date line (design/components.md, "Date line and region filter"): the date with its weekday on
// the left ("Wednesday 6 November 2024", the only place the weekday appears), anything passed as children (the
// region filter) on the right, and a hairline underneath. A page-template part. The date comes from the
// curation's clock, formatted by the shared formatter.
import type { ReactNode } from "react";
import { formatDateWithWeekday } from "../../format";

export interface DateLineProps {
  /** YYYY-MM-DD, the edition's date in the site's time zone. */
  date: string;
  children?: ReactNode;
}

export function DateLine({ date, children }: DateLineProps) {
  return (
    <div className="date-line">
      <p className="date-line__date">
        <time dateTime={date}>{formatDateWithWeekday(date)}</time>
      </p>
      {children ? <div className="date-line__end">{children}</div> : null}
    </div>
  );
}
