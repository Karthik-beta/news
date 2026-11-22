// The empty and error states (design/components.md, "Empty state", "Error state"): one sentence in Literata that
// says what is empty or what happened and what to do, plus one link onwards. No illustration, no icon, no colour:
// crimson is for form errors only. The words come from the template, never from data.
export interface StateMessageProps {
  message: string;
  action?: { href: string; label: string };
}

export function StateMessage({ message, action }: StateMessageProps) {
  return (
    <div className="state-message">
      <p className="state-message__text">{message}</p>
      {action ? (
        <p className="state-message__action">
          <a href={action.href}>{action.label}</a>
        </p>
      ) : null}
    </div>
  );
}
