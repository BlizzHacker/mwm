// KdbxWeb's browser path uses WebCrypto. The Node fallback is unreachable.
export function createHash() { throw new Error("WebCrypto is required"); }
export function createHmac() { throw new Error("WebCrypto is required"); }
export function randomBytes() { throw new Error("WebCrypto is required"); }
