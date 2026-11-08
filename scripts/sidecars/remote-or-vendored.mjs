import fs from 'node:fs';
import path from 'node:path';

/**
 * Fetches release metadata bytes over HTTPS, or reads the vendored copy when the
 * remote is unavailable (e.g. GitHub transient 504 on small release files).
 */
export async function fetchBytesOrVendored(fetchBytes, url, vendoredRel, root, onFallback) {
  try {
    return await fetchBytes(url);
  } catch (err) {
    if (!vendoredRel) {
      throw err;
    }
    if (onFallback) {
      onFallback(url, err, vendoredRel);
    }
    return fs.readFileSync(path.join(root, vendoredRel));
  }
}
