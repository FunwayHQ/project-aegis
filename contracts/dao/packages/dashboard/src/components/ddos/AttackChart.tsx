import { useMemo } from 'react';
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from 'recharts';
import type { AttackEvent } from '@aegis/ddos-sdk';

interface AttackChartProps {
  attacks: AttackEvent[];
}

export default function AttackChart({ attacks }: AttackChartProps) {
  // Group attacks by minute for the chart
  const chartData = useMemo(() => {
    const now = Date.now();
    const minutes: Record<string, { time: string; syn: number; udp: number; http: number }> = {};

    // Create buckets for the last 30 minutes
    for (let i = 29; i >= 0; i--) {
      const time = new Date(now - i * 60 * 1000);
      const key = time.toISOString().slice(0, 16); // YYYY-MM-DDTHH:MM
      minutes[key] = {
        time: time.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
        syn: 0,
        udp: 0,
        http: 0,
      };
    }

    // Count attacks per minute
    attacks.forEach((attack) => {
      const time = new Date(attack.timestamp * 1000);
      const key = time.toISOString().slice(0, 16);
      if (minutes[key]) {
        switch (attack.attack_type) {
          case 'syn_flood':
            minutes[key].syn++;
            break;
          case 'udp_flood':
            minutes[key].udp++;
            break;
          case 'http_flood':
          case 'slowloris':
            minutes[key].http++;
            break;
        }
      }
    });

    return Object.values(minutes);
  }, [attacks]);

  if (attacks.length === 0) {
    return (
      <div className="h-64 flex items-center justify-center text-gray-500">
        No attack data available
      </div>
    );
  }

  return (
    <div className="h-64">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={chartData}>
          <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
          <XAxis
            dataKey="time"
            stroke="#9CA3AF"
            fontSize={12}
            tickLine={false}
          />
          <YAxis
            stroke="#9CA3AF"
            fontSize={12}
            tickLine={false}
            allowDecimals={false}
          />
          <Tooltip
            contentStyle={{
              backgroundColor: '#1F2937',
              border: '1px solid #374151',
              borderRadius: '8px',
            }}
            labelStyle={{ color: '#F9FAFB' }}
          />
          <Legend />
          <Line
            type="monotone"
            dataKey="syn"
            name="SYN Flood"
            stroke="#EF4444"
            strokeWidth={2}
            dot={false}
          />
          <Line
            type="monotone"
            dataKey="udp"
            name="UDP Flood"
            stroke="#F59E0B"
            strokeWidth={2}
            dot={false}
          />
          <Line
            type="monotone"
            dataKey="http"
            name="HTTP Flood"
            stroke="#3B82F6"
            strokeWidth={2}
            dot={false}
          />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
