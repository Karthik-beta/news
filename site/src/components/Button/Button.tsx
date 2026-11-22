// Buttons (design/components.md, "Buttons"): primary (ink fill, the one form submit on a screen), secondary (paper
// fill, control border) and text (underlined, for actions under charts). A native <button>, so keyboard, focus
// ring and form submission come from the browser. Disabled is 40% opacity; put the reason in text nearby.
import type { ButtonHTMLAttributes } from "react";

export type ButtonVariant = "primary" | "secondary" | "text";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
}

export function Button({ variant = "secondary", type = "button", className, ...rest }: ButtonProps) {
  return <button type={type} className={`button button--${variant}${className ? ` ${className}` : ""}`} {...rest} />;
}
