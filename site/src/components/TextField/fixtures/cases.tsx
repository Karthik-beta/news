import type { FixtureCase } from "../../contract";
import { TextField } from "../TextField";

export const cases: FixtureCase[] = [
  { name: "plain, no marker", render: () => <TextField id="fixture-tf-plain" label="Company name" /> },
  {
    name: "required, with a hint",
    render: () => (
      <TextField
        id="fixture-tf-required"
        label="Page address"
        hint="The address in your browser's address bar."
        required
      />
    ),
  },
  {
    name: "optional email, filled in",
    render: () => (
      <TextField
        id="fixture-tf-optional"
        type="email"
        label="Your email, if you'd like a reply"
        required={false}
        autoComplete="email"
        value="reader@example.com"
      />
    ),
  },
  {
    name: "error",
    render: () => (
      <TextField
        id="fixture-tf-error"
        type="email"
        label="Your email, if you'd like a reply"
        required={false}
        value="reader@example"
        error="Enter an email address like name@example.com, or leave it empty."
      />
    ),
  },
  {
    name: "long label, hint and error wrap",
    render: () => (
      <TextField
        id="fixture-tf-long"
        label="Which figure, chart or table on the page looks wrong to you, and where did you see it?"
        hint="For example the chart title, the row of the table or the sentence the figure is in. Pneumonoultramicroscopicsilicovolcanoconiosis."
        required
        error="Say which figure looks wrong, so it can be checked against the source document and its page number."
      />
    ),
  },
];
