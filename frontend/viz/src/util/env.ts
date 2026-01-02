// @ts-nocheck

export function isServerMode() {
  const proto = window.location && window.location.protocol;
  return proto === "http:" || proto === "https:";
}
