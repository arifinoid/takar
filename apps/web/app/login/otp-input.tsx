"use client";

import { useRef } from "react";
import type { ChangeEvent, ClipboardEvent, KeyboardEvent } from "react";

export const OTP_LENGTH = 6;

type Props = {
  value: string[];
  onChange: (value: string[]) => void;
  label: string;
  disabled?: boolean;
};

export function OtpInput({ value, onChange, label, disabled }: Props) {
  const refs = useRef<Array<HTMLInputElement | null>>([]);

  function focus(index: number) {
    const input = refs.current[Math.min(Math.max(index, 0), OTP_LENGTH - 1)];
    input?.focus();
    input?.select();
  }

  function setCell(index: number, digit: string) {
    const next = [...value];
    next[index] = digit;
    onChange(next);
  }

  function handleChange(index: number, event: ChangeEvent<HTMLInputElement>) {
    const digits = event.target.value.replace(/\D/g, "");

    if (digits.length === 0) {
      setCell(index, "");
      return;
    }

    setCell(index, digits[digits.length - 1]);
    focus(index + 1);
  }

  function handleKeyDown(index: number, event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Backspace") {
      if (value[index]) {
        setCell(index, "");
        return;
      }

      event.preventDefault();
      if (index > 0) {
        setCell(index - 1, "");
        focus(index - 1);
      }
      return;
    }

    if (event.key === "ArrowLeft") {
      event.preventDefault();
      focus(index - 1);
    }

    if (event.key === "ArrowRight") {
      event.preventDefault();
      focus(index + 1);
    }
  }

  function handlePaste(event: ClipboardEvent<HTMLInputElement>) {
    const digits = event.clipboardData
      .getData("text")
      .replace(/\D/g, "")
      .slice(0, OTP_LENGTH);

    if (digits.length === 0) {
      return;
    }

    event.preventDefault();
    onChange(
      Array.from({ length: OTP_LENGTH }, (_, index) => digits[index] ?? ""),
    );
    focus(digits.length);
  }

  return (
    <div className="otp" role="group" aria-label={label}>
      {value.map((digit, index) => (
        <input
          key={index}
          ref={(element) => {
            refs.current[index] = element;
          }}
          className={digit ? "otp__box otp__box--filled" : "otp__box"}
          value={digit}
          onChange={(event) => handleChange(index, event)}
          onKeyDown={(event) => handleKeyDown(index, event)}
          onPaste={handlePaste}
          onFocus={(event) => event.target.select()}
          inputMode="numeric"
          autoComplete="one-time-code"
          maxLength={1}
          disabled={disabled}
          aria-label={`${label} ${index + 1}`}
        />
      ))}
    </div>
  );
}
