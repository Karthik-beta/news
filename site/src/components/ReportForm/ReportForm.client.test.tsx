// The report form island: JSON post to the intake, the honeypot, the page field and the intake's refusals.
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import ReportFormIsland, { failureOf, type ReportMessages } from "./ReportForm.client";

const messages: ReportMessages = {
  sent: "Report sent",
  sentDetail: "Your report has been received.",
  textTooShort: "Too short.",
  textTooLong: "Too long.",
  whereTooLong: "Where is too long.",
  emailNotValid: "Email not valid.",
  pageNotFound: "No published page has this address.",
  tooManyFromSender: "Too many in a minute.",
  overDailyLimit: "Closed until 00:00 UTC.",
  notSent: "Not sent.",
};
const API = "https://api.test/v1/reports";
const ORIGIN = "https://news.karthik.nz";
const REPORT = "The table says 3.1 but the source says 3.2.";

const setup = (search = "") => {
  window.history.replaceState(null, "", `/report/${search}`);
  render(<ReportFormIsland apiUrl={API} siteOrigin={ORIGIN} messages={messages} />);
  return userEvent.setup();
};

let fetchMock: ReturnType<typeof vi.fn>;
beforeEach(() => {
  fetchMock = vi.fn(async () => new Response(JSON.stringify({ status: "received" }), { status: 202 }));
  vi.stubGlobal("fetch", fetchMock);
});
afterEach(() => vi.unstubAllGlobals());

const sentBody = () => JSON.parse((fetchMock.mock.calls[0] as [string, RequestInit])[1].body as string);

describe("sending", () => {
  it("posts JSON to the intake with what as text, and shows the confirmation text", async () => {
    const user = setup();
    await user.type(screen.getByLabelText(/Address of the page/), `${ORIGIN}/2025/01/us-cpi-december-2024`);
    await user.type(screen.getByLabelText(/What is wrong/), REPORT);
    await user.type(screen.getByLabelText(/Where on the page/), "The chart title");
    await user.click(screen.getByRole("button", { name: "Send report" }));
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Report sent"));
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(API);
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "content-type": "application/json" });
    expect(sentBody()).toEqual({ page_id: "us-cpi-december-2024", text: REPORT, where: "The chart title" });
    expect(screen.getByRole("status").textContent).toContain("Your report has been received.");
    expect(screen.getByRole("link", { name: "Back to the page" }).getAttribute("href")).toBe(
      "/2025/01/us-cpi-december-2024",
    );
  });

  it("takes the page from ?page=<id>", async () => {
    const user = setup("?page=us-cpi-december-2024");
    await waitFor(() =>
      expect((screen.getByLabelText(/Address of the page/) as HTMLInputElement).value).toBe("us-cpi-december-2024"),
    );
    await user.type(screen.getByLabelText(/What is wrong/), REPORT);
    await user.click(screen.getByRole("button", { name: "Send report" }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(sentBody().page_id).toBe("us-cpi-december-2024");
  });

  it("sends the honeypot when a bot has filled it", async () => {
    const user = setup("?page=us-cpi");
    await waitFor(() =>
      expect((screen.getByLabelText(/Address of the page/) as HTMLInputElement).value).toBe("us-cpi"),
    );
    await user.type(screen.getByLabelText("Leave this field empty"), "http://spam.example");
    await user.type(screen.getByLabelText(/What is wrong/), REPORT);
    await user.click(screen.getByRole("button", { name: "Send report" }));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    expect(sentBody().website).toBe("http://spam.example");
  });
});

describe("checks before sending", () => {
  it("an empty report, a short one, a bad e-mail and an unreadable page send nothing, and say why", async () => {
    const user = setup();
    await user.type(screen.getByLabelText(/Address of the page/), "not a page");
    await user.type(screen.getByLabelText(/What is wrong/), "short");
    await user.type(screen.getByLabelText(/Your email/), "nope");
    await user.click(screen.getByRole("button", { name: "Send report" }));
    expect(fetchMock).not.toHaveBeenCalled();
    const summary = screen.getByText("The report was not sent").parentElement?.textContent ?? "";
    for (const text of [messages.pageNotFound, messages.textTooShort, messages.emailNotValid])
      expect(summary).toContain(text);
  });
});

describe("the intake's refusals", () => {
  const res = (status: number, body: unknown, headers: Record<string, string> = {}) =>
    new Response(JSON.stringify(body), { status, headers });

  it.each([
    [res(404, { error: { code: "unknown_page" } }), { errors: { page: messages.pageNotFound } }],
    [res(429, { error: { code: "rate_limited" } }, { "retry-after": "60" }), { formError: messages.tooManyFromSender }],
    [res(429, { error: { code: "rate_limited" } }, { "retry-after": "21600" }), { formError: messages.overDailyLimit }],
    [
      res(400, { error: { code: "invalid_request", details: [{ path: "/email" }, { path: "/where" }] } }),
      { errors: { email: messages.emailNotValid, where: messages.whereTooLong } },
    ],
    [res(500, "<html>"), { formError: messages.notSent }],
  ])("%#", async (response, expected) => {
    expect(await failureOf(response, { what: REPORT }, messages)).toEqual(expected);
  });

  it("a network failure keeps the form and says it was not sent", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("offline"));
    const user = setup("?page=us-cpi");
    await waitFor(() =>
      expect((screen.getByLabelText(/Address of the page/) as HTMLInputElement).value).toBe("us-cpi"),
    );
    await user.type(screen.getByLabelText(/What is wrong/), REPORT);
    await user.click(screen.getByRole("button", { name: "Send report" }));
    expect((await screen.findByRole("alert")).textContent).toContain(messages.notSent);
    expect((screen.getByLabelText(/What is wrong/) as HTMLTextAreaElement).value).toBe(REPORT);
  });
});
