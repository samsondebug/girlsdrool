import { useState } from "react";

import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Chip } from "../components/Chip";
import { EmptyState } from "../components/EmptyState";
import { ForecastChart } from "../components/ForecastChart";
import { Money } from "../components/Money";
import { Panel } from "../components/Panel";
import { TextField } from "../components/TextField";
import { MarkedMoney } from "../components/Untrusted";
import type {
  CategoryModel,
  Forecast as ForecastData,
  ForecastDay,
  ForecastEvent,
  ForecastWeek,
  Scenario,
} from "../lib/ipc";
import { formatCents, parseCentsInput } from "../lib/money";
import { useForecast, useSaveForecastPlan, useSetVariableOverride } from "../lib/queries";
import { BASELINE, useUiStore } from "../lib/store";

/**
 * The 91-day forecast: scenario inputs, the chart with its lowest point, the 30-day and 13-week
 * tables, and the variable-spend model with its overrides. Every figure comes from the core.
 */
export function Forecast() {
  const scenario = useUiStore((s) => s.forecastScenario);
  const setScenario = useUiStore((s) => s.setForecastScenario);
  const pushNotice = useUiStore((s) => s.pushNotice);
  const forecast = useForecast(scenario);
  const baseline = useForecast(BASELINE);
  const savePlan = useSaveForecastPlan();
  const [view, setView] = useState<"days" | "weeks">("days");
  const isBaseline = !scenario.downside && scenario.surprise_bill === null;
  const data = forecast.data;
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-4">
      <Panel
        title="Scenario"
        className="shrink-0"
        aside={
          <Button
            variant="quiet"
            disabled={savePlan.isPending}
            onClick={() => {
              savePlan.mutate(undefined, {
                onSuccess: (plan) => {
                  pushNotice({
                    tone: "positive",
                    text: `Plan saved as of ${plan.civil_date}; later forecasts are drawn against it.`,
                  });
                },
                onError: fail,
              });
            }}
          >
            Save baseline as plan
          </Button>
        }
      >
        <div className="flex flex-wrap items-end gap-4">
          <Checkbox
            label="Downside: next confirmed pay lands 7 days late"
            checked={scenario.downside}
            onChange={(e) => {
              setScenario({ ...scenario, downside: e.target.checked });
            }}
          />
          <SurpriseBillForm scenario={scenario} asOf={data?.as_of ?? ""} onChange={setScenario} />
          {isBaseline ? (
            <Chip tone="info">baseline</Chip>
          ) : (
            <Button
              variant="quiet"
              onClick={() => {
                setScenario(BASELINE);
              }}
            >
              Back to baseline
            </Button>
          )}
        </div>
        <p className="mt-2 text-12 text-text-dim">
          Only confirmed streams are income; expected and rumored ones never enter. Nothing is
          invented to avoid a low point. Committed = earmarks projected by their schedules + the
          timing buffer; headroom = closing − committed.
        </p>
      </Panel>

      {data ? (
        <>
          <Panel
            title={`${String(data.horizon_days + 1)} days from ${data.as_of}`}
            className="shrink-0"
          >
            <Summary data={data} baseline={isBaseline ? null : (baseline.data ?? null)} />
            <div className="mt-2 overflow-x-auto">
              <ForecastChart
                days={data.days}
                lowest={data.lowest}
                plan={data.plan?.days ?? null}
                width={1140}
                height={300}
              />
            </div>
            <p className="text-12 text-text-dim">
              Solid: closing balance. Dashed: committed. Red dot: the lowest point.
              {data.plan
                ? ` Dotted: the plan saved on ${data.plan.civil_date}.`
                : " Save the baseline as a plan to draw later forecasts against it."}
            </p>
          </Panel>

          <Panel
            title={view === "days" ? "Next 30 days" : "13 weeks"}
            className="shrink-0"
            aside={
              <div className="flex gap-1">
                <Button
                  variant={view === "days" ? "secondary" : "quiet"}
                  aria-pressed={view === "days"}
                  onClick={() => {
                    setView("days");
                  }}
                >
                  30 days
                </Button>
                <Button
                  variant={view === "weeks" ? "secondary" : "quiet"}
                  aria-pressed={view === "weeks"}
                  onClick={() => {
                    setView("weeks");
                  }}
                >
                  13 weeks
                </Button>
              </div>
            }
          >
            {view === "days" ? (
              <DaysTable days={data.days.slice(0, 30)} />
            ) : (
              <WeeksTable weeks={data.weeks} />
            )}
          </Panel>

          <Panel title="Variable spend model" className="shrink-0">
            <ModelTable model={data.model} total={data.model_total_cents} />
          </Panel>
        </>
      ) : forecast.error ? (
        <EmptyState
          missing={forecast.error.message}
          fix="Change the scenario inputs; a surprise bill must fall inside the 91 days and be a positive amount."
        />
      ) : null}
    </div>
  );
}

function Summary({ data, baseline }: { data: ForecastData; baseline: ForecastData | null }) {
  const untrustedBy = data.trust.hero.untrusted.map((u) => u.account_name);
  return (
    <div className="flex flex-wrap items-baseline gap-x-6 gap-y-2 text-14">
      <p className="flex items-baseline gap-2">
        <span className="text-text-dim">Lowest</span>
        <MarkedMoney cents={data.lowest.cents} size={20} untrustedBy={untrustedBy} />
        <span className="money text-12 text-text-dim">on {data.lowest.date}</span>
        {baseline ? (
          <span className="money text-12 text-text-dim">
            (baseline {formatCents(baseline.lowest.cents)} on {baseline.lowest.date})
          </span>
        ) : null}
      </p>
      {data.first_shortfall ? (
        <p className="flex items-center gap-2">
          <Chip tone="negative">first shortfall</Chip>
          <span className="money">{data.first_shortfall.date}</span>
          <Money cents={data.first_shortfall.cents} />
        </p>
      ) : null}
      {data.first_buffer_breach ? (
        <p className="flex items-center gap-2">
          <Chip tone="warning">first buffer breach</Chip>
          <span className="money">{data.first_buffer_breach.date}</span>
          <Money cents={data.first_buffer_breach.cents} />
        </p>
      ) : null}
      {!data.first_shortfall && !data.first_buffer_breach ? (
        <Chip tone="positive">no shortfall, no buffer breach</Chip>
      ) : null}
      <p className="flex items-baseline gap-2">
        <span className="text-text-dim">Opening</span>
        <Money cents={data.opening_cents} />
        <span className="text-text-dim">+ inflows</span>
        <Money cents={data.inflows_cents} tone={false} />
        <span className="text-text-dim">− outflows</span>
        <Money cents={data.outflows_cents} tone={false} />
        <span className="text-text-dim">= day {String(data.horizon_days)}</span>
        <MarkedMoney cents={data.closing_cents} untrustedBy={untrustedBy} />
      </p>
      <p className="flex items-baseline gap-2">
        <span className="text-text-dim">Pay dates</span>
        {data.pay_dates.length === 0 ? (
          <span className="text-text-dim">none confirmed</span>
        ) : (
          data.pay_dates.map((p) => (
            <span key={`${String(p.stream_id)}-${p.date}`} className="money text-12">
              {p.date.slice(5)}
              {p.shifted ? <Chip tone="warning">+7</Chip> : null}
            </span>
          ))
        )}
      </p>
    </div>
  );
}

function SurpriseBillForm({
  scenario,
  asOf,
  onChange,
}: {
  scenario: Scenario;
  asOf: string;
  onChange: (scenario: Scenario) => void;
}) {
  const current = scenario.surprise_bill;
  const [date, setDate] = useState(current?.date ?? asOf);
  const [text, setText] = useState(current ? formatCents(current.cents, { symbol: false }) : "");
  const cents = parseCentsInput(text);
  const valid = cents !== null && cents > 0 && date.length === 10;
  return (
    <form
      className="flex items-end gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (valid) onChange({ ...scenario, surprise_bill: { date, cents } });
      }}
    >
      <TextField
        label="Surprise bill on"
        type="date"
        mono
        className="w-40"
        value={date}
        onChange={(e) => {
          setDate(e.target.value);
        }}
      />
      <TextField
        label="Amount"
        mono
        className="w-32"
        value={text}
        placeholder="0.00"
        onChange={(e) => {
          setText(e.target.value);
        }}
        error={text !== "" && (cents === null || cents <= 0) ? "a positive amount" : null}
      />
      <Button type="submit" variant="secondary" disabled={!valid}>
        {current ? "Update bill" : "Add bill"}
      </Button>
      {current ? (
        <Button
          variant="quiet"
          onClick={() => {
            setText("");
            onChange({ ...scenario, surprise_bill: null });
          }}
        >
          Remove bill
        </Button>
      ) : null}
    </form>
  );
}

const EVENT_TONE: Record<ForecastEvent["kind"], "dim" | "positive" | "negative" | "warning"> = {
  pending: "dim",
  income: "positive",
  obligation: "negative",
  variable: "dim",
  surprise: "warning",
};

function DaysTable({ days }: { days: ForecastDay[] }) {
  return (
    <table className="w-full text-14">
      <thead className="text-12 text-text-dim">
        <tr className="border-b border-line text-left">
          <th className="py-1 pr-2 font-medium">Day</th>
          <th className="py-1 pr-2 font-medium">Date</th>
          <th className="py-1 pr-2 text-right font-medium">Inflows</th>
          <th className="py-1 pr-2 text-right font-medium">Outflows</th>
          <th className="py-1 pr-2 text-right font-medium">Closing</th>
          <th className="py-1 pr-2 text-right font-medium">Headroom</th>
          <th className="py-1 font-medium">Events</th>
        </tr>
      </thead>
      <tbody>
        {days.map((d) => {
          const variable = d.events
            .filter((e) => e.kind === "variable")
            .reduce((sum, e) => sum + e.cents, 0);
          return (
            <tr
              key={d.day}
              className={`border-b border-line ${d.closing_cents < 0 ? "text-negative" : ""}`}
            >
              <td className="money py-1 pr-2 text-text-dim">{d.day}</td>
              <td className="money py-1 pr-2">{d.date}</td>
              <td className="py-1 pr-2 text-right">
                {d.inflows_cents === 0 ? (
                  <span className="text-text-dim">—</span>
                ) : (
                  <Money cents={d.inflows_cents} tone={false} />
                )}
              </td>
              <td className="py-1 pr-2 text-right">
                <Money cents={d.outflows_cents} tone={false} />
              </td>
              <td className="py-1 pr-2 text-right">
                <Money cents={d.closing_cents} />
              </td>
              <td className="py-1 pr-2 text-right">
                <Money cents={d.headroom_cents} />
              </td>
              <td className="py-1">
                <span className="flex flex-wrap gap-1">
                  {d.events
                    .filter((e) => e.kind !== "variable")
                    .map((e, i) => (
                      <Chip key={`${e.kind}-${String(e.ref_id ?? i)}`} tone={EVENT_TONE[e.kind]}>
                        {e.name} {formatCents(e.cents, { sign: "always" })}
                      </Chip>
                    ))}
                  {variable !== 0 ? <Chip tone="dim">variable {formatCents(variable)}</Chip> : null}
                </span>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function WeeksTable({ weeks }: { weeks: ForecastWeek[] }) {
  return (
    <table className="w-full text-14">
      <thead className="text-12 text-text-dim">
        <tr className="border-b border-line text-left">
          <th className="py-1 pr-2 font-medium">Week</th>
          <th className="py-1 pr-2 font-medium">From</th>
          <th className="py-1 pr-2 font-medium">To</th>
          <th className="py-1 pr-2 text-right font-medium">Inflows</th>
          <th className="py-1 pr-2 text-right font-medium">Outflows</th>
          <th className="py-1 pr-2 text-right font-medium">Closing</th>
          <th className="py-1 text-right font-medium">Lowest</th>
        </tr>
      </thead>
      <tbody>
        {weeks.map((w) => (
          <tr
            key={w.week}
            className={`border-b border-line ${w.lowest_cents < 0 ? "text-negative" : ""}`}
          >
            <td className="money py-1 pr-2 text-text-dim">{w.week + 1}</td>
            <td className="money py-1 pr-2">{w.start}</td>
            <td className="money py-1 pr-2">{w.end}</td>
            <td className="py-1 pr-2 text-right">
              <Money cents={w.inflows_cents} tone={false} />
            </td>
            <td className="py-1 pr-2 text-right">
              <Money cents={w.outflows_cents} tone={false} />
            </td>
            <td className="py-1 pr-2 text-right">
              <Money cents={w.closing_cents} />
            </td>
            <td className="py-1 text-right">
              <Money cents={w.lowest_cents} />
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function ModelTable({ model, total }: { model: CategoryModel[]; total: number }) {
  const first = model[0];
  return (
    <div className="flex flex-col gap-2 text-14">
      <p className="text-12 text-text-dim">
        Per category: net outflow of posted, non-transfer rows in three 30-day buckets ending
        yesterday
        {first
          ? ` (${first.buckets[0]?.start ?? ""}..${first.buckets[0]?.end ?? ""}, ${first.buckets[1]?.start ?? ""}..${first.buckets[1]?.end ?? ""}, ${first.buckets[2]?.start ?? ""}..${first.buckets[2]?.end ?? ""})`
          : ""}
        , floored at zero; the model is the median. An override replaces the median for that
        category only. Each 30 forecast days spend the model exactly, by largest remainder.
      </p>
      <table className="w-full">
        <thead className="text-12 text-text-dim">
          <tr className="border-b border-line text-left">
            <th className="py-1 pr-2 font-medium">Category</th>
            <th className="py-1 pr-2 text-right font-medium">Bucket 1</th>
            <th className="py-1 pr-2 text-right font-medium">Bucket 2</th>
            <th className="py-1 pr-2 text-right font-medium">Bucket 3</th>
            <th className="py-1 pr-2 text-right font-medium">Median</th>
            <th className="py-1 pr-2 text-right font-medium">Per 30 days</th>
            <th className="py-1 font-medium">Override</th>
          </tr>
        </thead>
        <tbody>
          {model.map((m) => (
            <tr key={m.category_id} className="border-b border-line">
              <td className="py-1 pr-2">{m.name}</td>
              {m.buckets.map((b) => (
                <td key={b.start} className="py-1 pr-2 text-right">
                  <Money cents={b.net_outflow_cents} tone={false} />
                </td>
              ))}
              <td className="py-1 pr-2 text-right">
                <Money cents={m.median_cents} tone={false} />
              </td>
              <td className="py-1 pr-2 text-right">
                <span className="inline-flex items-center gap-2">
                  <Money cents={m.per_30_days_cents} tone={false} />
                  {m.override_cents !== null ? <Chip tone="info">override</Chip> : null}
                </span>
              </td>
              <td className="py-1">
                <OverrideForm key={m.override_cents ?? "none"} model={m} />
              </td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <td className="py-1 pr-2 text-text-dim" colSpan={5}>
              Σ model per 30 days
            </td>
            <td className="py-1 pr-2 text-right">
              <Money cents={total} tone={false} />
            </td>
            <td />
          </tr>
        </tfoot>
      </table>
    </div>
  );
}

function OverrideForm({ model }: { model: CategoryModel }) {
  const pushNotice = useUiStore((s) => s.pushNotice);
  const setOverride = useSetVariableOverride();
  const [text, setText] = useState(
    model.override_cents === null ? "" : formatCents(model.override_cents, { symbol: false }),
  );
  const cents = parseCentsInput(text);
  const fail = (error: Error) => {
    pushNotice({ tone: "negative", text: error.message });
  };
  return (
    <form
      className="flex items-center gap-1"
      onSubmit={(e) => {
        e.preventDefault();
        if (cents === null || cents < 0) return;
        setOverride.mutate(
          { categoryId: model.category_id, cents },
          {
            onSuccess: () => {
              pushNotice({
                tone: "info",
                text: `${model.name}: the forecast now spends ${formatCents(cents)} per 30 days.`,
              });
            },
            onError: fail,
          },
        );
      }}
    >
      <TextField
        label=""
        aria-label={`${model.name} override per 30 days`}
        mono
        className="w-28"
        value={text}
        placeholder="per 30 days"
        onChange={(e) => {
          setText(e.target.value);
        }}
        error={text !== "" && (cents === null || cents < 0) ? "not an amount" : null}
      />
      <Button
        type="submit"
        variant="quiet"
        disabled={
          setOverride.isPending || cents === null || cents < 0 || cents === model.override_cents
        }
      >
        Set
      </Button>
      {model.override_cents !== null ? (
        <Button
          variant="quiet"
          disabled={setOverride.isPending}
          onClick={() => {
            setOverride.mutate(
              { categoryId: model.category_id, cents: null },
              {
                onSuccess: () => {
                  pushNotice({ tone: "info", text: `${model.name}: back to the median.` });
                },
                onError: fail,
              },
            );
          }}
        >
          Clear
        </Button>
      ) : null}
    </form>
  );
}
