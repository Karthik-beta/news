// The report-an-error form (design/components.md, "Forms"; design/layout.md, "Report an error"): "What is wrong?"
// (required), "Where on the page?" (optional), an optional email and one primary "Send report" button. A plain
// POST to `action`, so it works with no JavaScript: the intake (issue #14) validates with `reportErrors`, and on a
// problem renders the form again with the values and errors; on success it renders it with `sent`.
import type { SubmitEvent as ReactSubmitEvent } from "react";
import { Button } from "../Button/Button";
import { Icon } from "../Icon/Icon";
import { TextArea } from "../TextArea/TextArea";
import { TextField } from "../TextField/TextField";

export interface ReportValues {
  /** The page's address or ID, when the form asks for it (`askPage`). */
  page?: string;
  what?: string;
  where?: string;
  email?: string;
}

export type ReportErrors = Partial<Record<keyof ReportValues, string>>;

/** The field ids, which the error summary links to. */
export const REPORT_FIELD_IDS: Record<keyof ReportValues, string> = {
  page: "report-page",
  what: "report-what",
  where: "report-where",
  email: "report-email",
};

// shortcut: a shape check only (something@something.tld); the intake decides what it accepts.
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** The form's own checks, in field order. Empty when the report can be sent. */
export function reportErrors(values: ReportValues): ReportErrors {
  const errors: ReportErrors = {};
  if (!values.what?.trim()) errors.what = "Say what is wrong.";
  const email = values.email?.trim();
  if (email && !EMAIL.test(email)) errors.email = "Enter an email address like name@example.com, or leave it empty.";
  return errors;
}

export interface ErrorSummaryProps {
  title: string;
  /** Field id and message, in the order of the fields. */
  errors: { id: string; message: string }[];
}

/** The summary at the top of a form: what to fix, each message a link to its field. Announced politely. */
export function ErrorSummary({ title, errors }: ErrorSummaryProps) {
  return (
    <div className="error-summary" aria-live="polite">
      {errors.length > 0 ? (
        <>
          <h2 className="error-summary__title">
            <Icon name="error" className="error-summary__icon" />
            {title}
          </h2>
          <ul className="error-summary__list">
            {errors.map((e) => (
              <li key={e.id}>
                <a className="error-summary__link" href={`#${e.id}`}>
                  {e.message}
                </a>
              </li>
            ))}
          </ul>
        </>
      ) : null}
    </div>
  );
}

export interface ReportFormProps {
  /** Where the form posts: the error-report intake (issue #14). */
  action: string;
  /** The page being reported, sent as the hidden field `page` and linked back to once the report is sent. Only site-relative paths are kept; anything else falls back to "/". */
  pageUrl: string;
  /** The values the form was last sent with, so a page with errors keeps what the reader wrote. */
  values?: ReportValues;
  /** Errors from `reportErrors` or the intake. */
  errors?: ReportErrors;
  /** The report went through: show the confirmation instead of the form. */
  sent?: boolean;
  /** The confirmation's words, when the page has its own (the copy's "After sending" rows). */
  sentText?: { title: string; detail?: string };
  /** Ask for the page's address in a field (a page with no page named in its address) instead of sending `pageUrl`. */
  askPage?: boolean;
  /** Set by the island that sends the form with JavaScript: it takes over the submit. */
  onSubmit?: (event: ReactSubmitEvent<HTMLFormElement>) => void;
  /** The report is on its way: the button waits. */
  submitting?: boolean;
  /** A problem that belongs to no field (the report could not be sent). */
  formError?: string;
}

export function ReportForm({
  action,
  pageUrl: rawPageUrl,
  values = {},
  errors = {},
  sent = false,
  sentText,
  askPage = false,
  onSubmit,
  submitting = false,
  formError,
}: ReportFormProps) {
  const pageUrl = /^\/(?!\/)/.test(rawPageUrl) ? rawPageUrl : "/";
  if (sent) {
    return (
      <div className="report-form report-form--sent" role="status">
        <p className="report-form__sent">
          <Icon name="check" className="report-form__sent-icon" />
          {sentText?.title ?? "Report sent. Thank you for checking."}
        </p>
        {sentText?.detail ? <p className="report-form__detail">{sentText.detail}</p> : null}
        <p className="report-form__back">
          <a href={pageUrl}>Back to the page</a>
        </p>
      </div>
    );
  }
  const listed = (Object.keys(REPORT_FIELD_IDS) as (keyof ReportValues)[]).flatMap((k) => {
    const message = errors[k];
    return message ? [{ id: REPORT_FIELD_IDS[k], message }] : [];
  });
  return (
    <form className="report-form" action={action} method="post" noValidate onSubmit={onSubmit}>
      <ErrorSummary title="The report was not sent" errors={listed} />
      {formError ? (
        <p className="field__error" role="alert">
          <Icon name="error" className="field__error-icon" />
          {formError}
        </p>
      ) : null}
      {askPage ? (
        <TextField
          id={REPORT_FIELD_IDS.page}
          name="page"
          label="Address of the page"
          hint="Copy it from your browser's address bar."
          required
          value={values.page}
          error={errors.page}
        />
      ) : (
        <input type="hidden" name="page" value={pageUrl} />
      )}
      {/* The honeypot: people never see or reach this field, so a filled one is a bot (workers/api reports.ts). */}
      <div className="visually-hidden" aria-hidden="true">
        <label>
          Leave this field empty
          <input type="text" name="website" tabIndex={-1} autoComplete="off" />
        </label>
      </div>
      <TextArea
        id={REPORT_FIELD_IDS.what}
        name="what"
        label="What is wrong?"
        required
        value={values.what}
        error={errors.what}
      />
      <TextField
        id={REPORT_FIELD_IDS.where}
        name="where"
        label="Where on the page?"
        hint="For example the chart title, or the sentence the figure is in."
        required={false}
        value={values.where}
        error={errors.where}
      />
      <TextField
        id={REPORT_FIELD_IDS.email}
        name="email"
        type="email"
        label="Your email, if you'd like a reply"
        required={false}
        autoComplete="email"
        value={values.email}
        error={errors.email}
      />
      <div>
        <Button variant="primary" type="submit" disabled={submitting}>
          {submitting ? "Sending…" : "Send report"}
        </Button>
      </div>
    </form>
  );
}
