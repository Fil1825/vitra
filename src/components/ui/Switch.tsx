import styles from './controls.module.css';

export interface SwitchProps {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange(checked: boolean): void;
}

export function Switch({ label, checked, disabled, onChange }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={styles.switch}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className={styles.thumb} />
    </button>
  );
}
