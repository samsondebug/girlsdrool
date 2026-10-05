import { AxisBottom, AxisLeft } from "@visx/axis";
import { curveMonotoneX } from "@visx/curve";
import { Group } from "@visx/group";
import { scaleLinear } from "@visx/scale";
import { LinePath } from "@visx/shape";

import type { ForecastDay, ForecastPoint } from "../lib/ipc";
import { formatCents } from "../lib/money";

interface ForecastChartProps {
  days: ForecastDay[];
  lowest: ForecastPoint;
  /** A saved plan's closings, drawn dimmed against the live series. */
  plan?: ForecastPoint[] | null;
  width: number;
  height: number;
  /** Sparkline: no axes, no labels, scaled to its container. */
  compact?: boolean;
}

const TICK_DAYS = [0, 14, 28, 42, 56, 70, 84, 90];

/**
 * The closing balance per day (solid), the committed line (dashed, warning tone), the zero line
 * when the series dips below it, the plan overlay when one is saved, and the lowest point marked.
 * Colors are the tokens, through Tailwind's stroke and fill utilities; nothing is computed here.
 */
export function ForecastChart({
  days,
  lowest,
  plan = null,
  width,
  height,
  compact = false,
}: ForecastChartProps) {
  const margin = compact
    ? { top: 4, right: 4, bottom: 4, left: 4 }
    : { top: 16, right: 16, bottom: 28, left: 84 };
  const innerW = Math.max(0, width - margin.left - margin.right);
  const innerH = Math.max(0, height - margin.top - margin.bottom);
  const last = Math.max(1, days.length - 1);
  const planPoints = plan ? plan.slice(0, days.length) : [];
  const values = days.flatMap((d) => [d.closing_cents, d.committed_cents]);
  for (const p of planPoints) values.push(p.cents);
  values.push(0);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const x = scaleLinear<number>({ domain: [0, last], range: [0, innerW] });
  const y = scaleLinear<number>({ domain: [min, max], range: [innerH, 0], nice: true });
  const lowestIndex = days.findIndex((d) => d.date === lowest.date);
  const labelAbove = y(lowest.cents) > innerH / 2;
  return (
    <svg
      width={compact ? "100%" : width}
      height={compact ? "100%" : height}
      viewBox={`0 0 ${String(width)} ${String(height)}`}
      preserveAspectRatio={compact ? "none" : "xMinYMin meet"}
      role="img"
      aria-label={`Forecast: lowest ${formatCents(lowest.cents)} on ${lowest.date}`}
      className="text-text-dim"
    >
      <Group left={margin.left} top={margin.top}>
        {min < 0 ? (
          <line
            x1={0}
            x2={innerW}
            y1={y(0)}
            y2={y(0)}
            className="stroke-negative"
            strokeDasharray="2 3"
          />
        ) : null}
        {planPoints.length > 0 ? (
          <LinePath
            data={planPoints}
            x={(_, i) => x(i)}
            y={(p) => y(p.cents)}
            className="stroke-text-dim"
            strokeWidth={1}
            strokeDasharray="4 3"
            fill="none"
            curve={curveMonotoneX}
          />
        ) : null}
        <LinePath
          data={days}
          x={(d) => x(d.day)}
          y={(d) => y(d.committed_cents)}
          className="stroke-warning"
          strokeWidth={1}
          strokeDasharray="3 3"
          fill="none"
        />
        <LinePath
          data={days}
          x={(d) => x(d.day)}
          y={(d) => y(d.closing_cents)}
          className="stroke-text"
          strokeWidth={compact ? 1.5 : 2}
          fill="none"
          curve={curveMonotoneX}
        />
        {lowestIndex >= 0 ? (
          <circle
            cx={x(lowestIndex)}
            cy={y(lowest.cents)}
            r={compact ? 2.5 : 4}
            className="fill-negative"
          />
        ) : null}
        {!compact ? (
          <>
            <AxisLeft
              scale={y}
              numTicks={5}
              stroke="currentColor"
              tickStroke="currentColor"
              tickFormat={(v) => formatCents(Number(v))}
              tickLabelProps={() => ({
                fill: "currentColor",
                fontSize: 11,
                fontFamily: "var(--font-mono)",
                textAnchor: "end",
                dx: -4,
                dy: 3,
              })}
            />
            <AxisBottom
              top={innerH}
              scale={x}
              tickValues={TICK_DAYS.filter((t) => t <= last)}
              tickFormat={(v) => days[Number(v)]?.date.slice(5) ?? ""}
              stroke="currentColor"
              tickStroke="currentColor"
              tickLabelProps={() => ({
                fill: "currentColor",
                fontSize: 11,
                fontFamily: "var(--font-mono)",
                textAnchor: "middle",
                dy: 4,
              })}
            />
            {lowestIndex >= 0 ? (
              <text
                x={x(lowestIndex)}
                y={y(lowest.cents) + (labelAbove ? -10 : 16)}
                className="fill-negative"
                fontSize={11}
                fontFamily="var(--font-mono)"
                textAnchor={lowestIndex < 8 ? "start" : lowestIndex > last - 8 ? "end" : "middle"}
              >
                {formatCents(lowest.cents)} · {lowest.date}
              </text>
            ) : null}
          </>
        ) : null}
      </Group>
    </svg>
  );
}
