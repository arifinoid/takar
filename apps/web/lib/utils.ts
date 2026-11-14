import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Joins conditional classes and lets a caller's class win over the defaults. */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
