// A one-line text input (design/components.md, "Forms"). Server-rendered and uncontrolled: `value` is the value
// the form was last sent with, so a page re-rendered with errors keeps what the reader typed.
import { controlProps, Field, type FieldProps } from "./field";

export interface TextFieldProps extends FieldProps {
  type?: "text" | "email" | "url" | "tel";
  value?: string;
  autoComplete?: string;
}

export function TextField({ type = "text", value, autoComplete, ...field }: TextFieldProps) {
  return (
    <Field field={field}>
      <input
        className="field__control"
        type={type}
        defaultValue={value}
        autoComplete={autoComplete}
        spellCheck={type === "text" ? undefined : false}
        {...controlProps(field)}
      />
    </Field>
  );
}
