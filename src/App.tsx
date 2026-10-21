import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getSidecarStatus, type SidecarStatus } from './lib/sidecars.ts';
import styles from './App.module.css';

/** Stage 0 placeholder: shows which helper binaries the backend can find. */
export default function App() {
  const [sidecars, setSidecars] = useState<SidecarStatus[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getSidecarStatus()
      .then(setSidecars)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <main className={styles.app}>
      <section className={styles.card}>
        <h1 className={styles.title}>Vitra</h1>
        <p className={styles.subtitle}>Development build — stage 0</p>
        {!isTauri() && <p>Running in a plain browser: backend is not available.</p>}
        {error && <p className={styles.missing}>{error}</p>}
        {sidecars && (
          <ul className={styles.list}>
            {sidecars.map((s) => (
              <li key={s.name} className={styles.row}>
                <span className={styles.name}>{s.name}</span>
                {s.path ? (
                  <span className={styles.path} title={s.path}>
                    {s.source}: {s.path}
                  </span>
                ) : (
                  <span className={styles.missing}>not found</span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}
