import type { ReactNode } from 'react';

interface StatCardProps {
  title: string;
  value: number | string;
  subtitle?: string;
  icon?: ReactNode;
  color?: 'aegis' | 'red' | 'yellow' | 'green';
}

export default function StatCard({
  title,
  value,
  subtitle,
  icon,
  color = 'aegis',
}: StatCardProps) {
  const colorClasses = {
    aegis: 'text-aegis-400 bg-aegis-500/10',
    red: 'text-red-400 bg-red-500/10',
    yellow: 'text-yellow-400 bg-yellow-500/10',
    green: 'text-green-400 bg-green-500/10',
  };

  return (
    <div className="stat-card flex items-start gap-4">
      {icon && (
        <div className={`p-3 rounded-lg ${colorClasses[color]}`}>{icon}</div>
      )}
      <div className="flex-1">
        <p className="text-sm text-gray-400">{title}</p>
        <p className="text-2xl font-bold text-white">
          {typeof value === 'number' ? value.toLocaleString() : value}
        </p>
        {subtitle && <p className="text-xs text-gray-500 mt-1">{subtitle}</p>}
      </div>
    </div>
  );
}
