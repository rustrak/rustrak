import { useTranslations } from 'use-intl';

interface EventContextProps {
  contexts?: Record<string, Record<string, unknown>>;
  modules?: Record<string, string>;
  user?: {
    id?: string;
    email?: string;
    ip_address?: string;
  };
}

export function EventContext({ contexts, modules, user }: EventContextProps) {
  const t = useTranslations('events');
  const hasContexts = contexts && Object.keys(contexts).length > 0;
  const hasModules = modules && Object.keys(modules).length > 0;
  const hasUser = user && (user.id || user.email || user.ip_address);

  if (!hasContexts && !hasModules && !hasUser) {
    return (
      <div className="text-center py-12 text-fg-subtle">
        {t('context.empty')}
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* User Context */}
      {hasUser && (
        <ContextSection title={t('context.user')}>
          <div className="grid grid-cols-2 gap-4">
            {user.id && <ContextItem label={t('context.id')} value={user.id} />}
            {user.email && (
              <ContextItem label={t('context.email')} value={user.email} />
            )}
            {user.ip_address && (
              <ContextItem
                label={t('context.ipAddress')}
                value={user.ip_address}
              />
            )}
          </div>
        </ContextSection>
      )}

      {/* Other Contexts */}
      {contexts &&
        Object.entries(contexts).map(([key, value]) => (
          <ContextSection key={key} title={formatContextName(key)}>
            <div className="grid grid-cols-2 gap-4">
              {Object.entries(value).map(([k, v]) => (
                <ContextItem
                  key={k}
                  label={formatLabel(k)}
                  value={formatValue(v)}
                />
              ))}
            </div>
          </ContextSection>
        ))}

      {/* Packages (dependency versions running at capture time) */}
      {hasModules && (
        <ContextSection title={t('context.packages')}>
          <div className="grid grid-cols-2 gap-4">
            {Object.entries(modules)
              .sort(([a], [b]) => a.localeCompare(b))
              .map(([name, version]) => (
                <ContextItem key={name} label={name} value={version} />
              ))}
          </div>
        </ContextSection>
      )}
    </div>
  );
}

function ContextSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="bg-surface border rounded-lg p-6 space-y-4">
      <h4 className="text-column text-fg-subtle">{title}</h4>
      {children}
    </div>
  );
}

function ContextItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="space-y-1">
      <p className="text-column text-fg-subtle">{label}</p>
      <p className="text-sm font-mono whitespace-pre-wrap break-words">
        {value}
      </p>
    </div>
  );
}

function formatContextName(name: string): string {
  return name.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function formatLabel(key: string): string {
  return key.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function formatValue(value: unknown): string {
  if (value === null || value === undefined) return '-';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}
