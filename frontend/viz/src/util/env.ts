
export function isServerMode(): boolean {
  const proto = window.location && window.location.protocol;
  return proto === "http:" || proto === "https:";
}
