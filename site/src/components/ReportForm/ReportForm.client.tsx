// The report form's interactive part (an island): sends the form as JSON to the intake (POST /v1/reports on the api
// Worker, ADR 0010 section 8) and shows its answer. Without JavaScript the form cannot be sent: the page says so and
// points to the contact page. The words (limits, errors, confirmation) come in as props from the page's copy.
import { type SubmitEvent as ReactSubmitEvent, useEffect, useState } from "react";
import { backPath, intakeBody, type LimitMessages, limitErrors, pageIdOf } from "./intake";
import { type ReportErrors, ReportForm, type ReportValues, reportErrors } from "./ReportForm";

export interface ReportMessages extends LimitMessages {
  sent: string;
  sentDetail: string;
  emailNotValid: string;
  pageNotFound: string;
  tooManyFromSender: string;
  overDailyLimit: string;
  notSent: string;
}

export interface ReportFormIslandProps {
  /** The intake's full URL (`<NEWS_API_ORIGIN>/v1/reports`). */
  apiUrl: string;
  /** The site's origin, to read a pasted page address. */
  siteOrigin: string;
  messages: ReportMessages;
}

type Failure = { errors?: ReportErrors; formError?: string };

/** The copy's words for the form's own e-mail check. */
const mapEmail = (errors: ReportErrors, m: ReportMessages): ReportErrors => ({
  ...errors,
  ...(errors.email ? { email: m.emailNotValid } : {}),
});

/** What the intake's refusal means to a reader. Retry-After tells the minute limit (about 60 s) from the daily cap. */
export async function failureOf(res: Response, values: ReportValues, m: ReportMessages): Promise<Failure> {
  const body = (await res.json().catch(() => ({}))) as {
    error?: { code?: string; details?: { path?: string }[] };
  };
  const code = body.error?.code;
  if (res.status === 429) {
    const wait = Number(res.headers.get("retry-after") ?? 0);
    return { formError: wait > 120 ? m.overDailyLimit : m.tooManyFromSender };
  }
  if (code === "unknown_page") return { errors: { page: m.pageNotFound } };
  if (code === "invalid_request") {
    const paths = (body.error?.details ?? []).map((d) => d.path);
    const errors: ReportErrors = {};
    if (paths.includes("/page_id")) errors.page = m.pageNotFound;
    if (paths.includes("/text")) errors.what = limitErrors(values, m).what ?? m.textTooShort;
    if (paths.includes("/where")) errors.where = m.whereTooLong;
    if (paths.includes("/email")) errors.email = m.emailNotValid;
    if (Object.keys(errors).length > 0) return { errors };
  }
  return { formError: m.notSent };
}

export default function ReportFormIsland({ apiUrl, siteOrigin, messages }: ReportFormIslandProps) {
  // The page named in the address (`/report/?page=<id>`) is read after hydration, so the server markup matches.
  const [named, setNamed] = useState("");
  useEffect(() => setNamed(new URLSearchParams(location.search).get("page") ?? ""), []);
  const [values, setValues] = useState<ReportValues>({});
  const [errors, setErrors] = useState<ReportErrors>({});
  const [formError, setFormError] = useState<string>();
  const [submitting, setSubmitting] = useState(false);
  const [sent, setSent] = useState(false);

  async function submit(event: ReactSubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const data = new FormData(event.currentTarget);
    const field = (k: string) => String(data.get(k) ?? "");
    const typed: ReportValues = {
      page: field("page"),
      what: field("what"),
      where: field("where"),
      email: field("email"),
    };
    setValues(typed);
    setFormError(undefined);
    const pageId = pageIdOf(typed.page ?? "", siteOrigin);
    const found = {
      ...mapEmail(reportErrors(typed), messages),
      ...limitErrors(typed, messages),
      ...(pageId ? {} : { page: messages.pageNotFound }),
    };
    // The form's own order: page, what, where, email.
    const problems: ReportErrors = { page: found.page, what: found.what, where: found.where, email: found.email };
    for (const k of Object.keys(problems) as (keyof ReportErrors)[]) if (!problems[k]) delete problems[k];
    setErrors(problems);
    if (!pageId || Object.keys(problems).length > 0) return;
    setSubmitting(true);
    try {
      const res = await fetch(apiUrl, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(intakeBody(typed, pageId, field("website"))),
      });
      if (res.status === 202) setSent(true);
      else {
        const failure = await failureOf(res, typed, messages);
        setErrors(failure.errors ?? {});
        setFormError(failure.formError);
      }
    } catch {
      setFormError(messages.notSent);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <ReportForm
      // A named page arrives after hydration: a new key shows it in the (uncontrolled) field.
      key={named}
      action="/report/"
      pageUrl={backPath(values.page ?? named, siteOrigin)}
      values={{ ...values, page: values.page ?? named }}
      errors={errors}
      askPage
      sent={sent}
      sentText={{ title: messages.sent, detail: messages.sentDetail }}
      submitting={submitting}
      formError={formError}
      onSubmit={submit}
    />
  );
}
