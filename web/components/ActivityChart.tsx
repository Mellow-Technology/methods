"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LineChart, type LineSeriesOption } from "echarts/charts";
import {
  GridComponent,
  LegendComponent,
  TooltipComponent,
  type GridComponentOption,
  type LegendComponentOption,
  type TooltipComponentOption,
} from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import { Alert } from "gui-mellow/Alert";
import { PageHeader } from "gui-mellow/PageHeader";
import { SegmentedControl } from "gui-mellow/SegmentedControl";
import { THEME_CHANGE_EVENT } from "gui-mellow/theme";
import { api, errorMessage } from "@/lib/api";
import { useLiveRefresh } from "@/lib/live";
import styles from "./ActivityChart.module.scss";

echarts.use([LineChart, GridComponent, LegendComponent, TooltipComponent, CanvasRenderer]);

type Option = echarts.ComposeOption<
  LineSeriesOption | GridComponentOption | LegendComponentOption | TooltipComponentOption
>;

export interface DayStats {
  day: string; // YYYY-MM-DD, in the viewer's time zone
  created: number;
  started: number;
  completed: number;
  comments: number;
  other: number;
}

type Kind = Exclude<keyof DayStats, "day">;

// Bottom of the stack first. The order keeps red and green apart, which
// colour-blind readers can't tell from each other side by side.
const KINDS: { key: Kind; label: string; token: string }[] = [
  { key: "completed", label: "Completed", token: "--gm-color-success" },
  { key: "started", label: "Started", token: "--gm-color-warning" },
  { key: "created", label: "Created", token: "--gm-color-primary" },
  { key: "comments", label: "Comments", token: "--gm-color-accent" },
  { key: "other", label: "Other changes", token: "--gm-color-text-faint" },
];

const RANGES = [
  { value: "7", label: "Week", phrase: "the past week" },
  { value: "30", label: "Month", phrase: "the past 30 days" },
  { value: "90", label: "Quarter", phrase: "the past 90 days" },
] as const;
type Range = (typeof RANGES)[number]["value"];

type Rgb = [number, number, number];

interface Palette {
  series: Record<Kind, Rgb>;
  text: string;
  muted: string;
  border: string;
  font: string;
}

/**
 * Theme tokens as plain sRGB. The canvas can't read CSS variables, and the
 * theme defines some colours in oklch, which ECharts can't parse, so each
 * one is painted onto a 1×1 canvas and read back.
 */
function readPalette(el: HTMLElement): Palette {
  const css = getComputedStyle(el);
  const ctx = document.createElement("canvas").getContext("2d", { willReadFrequently: true })!;
  const rgb = (token: string): Rgb => {
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillStyle = "#000";
    ctx.fillStyle = css.getPropertyValue(token).trim() || "#888";
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
    return [r!, g!, b!];
  };
  const solid = (token: string) => `rgb(${rgb(token).join(",")})`;
  return {
    series: Object.fromEntries(KINDS.map((k) => [k.key, rgb(k.token)])) as Record<Kind, Rgb>,
    text: solid("--gm-color-text"),
    muted: solid("--gm-color-text-muted"),
    border: solid("--gm-color-border"),
    font: css.getPropertyValue("--gm-font-body").trim() || "system-ui, sans-serif",
  };
}

const rgba = ([r, g, b]: Rgb, a: number) => `rgba(${r},${g},${b},${a})`;

function parseDay(day: string) {
  return new Date(`${day}T00:00:00`);
}

function dayLabel(day: string, range: Range) {
  const d = parseDay(day);
  return range === "7"
    ? d.toLocaleDateString(undefined, { weekday: "short" })
    : d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function buildOption(days: DayStats[], range: Range, p: Palette, animate: boolean): Option {
  return {
    animation: animate,
    animationDuration: 500,
    textStyle: { fontFamily: p.font, color: p.muted },
    grid: { left: 4, right: 12, top: 40, bottom: 4, containLabel: true },
    legend: {
      top: 0,
      left: 0,
      icon: "roundRect",
      itemWidth: 10,
      itemHeight: 10,
      itemGap: 16,
      textStyle: { color: p.muted, fontSize: 12 },
      inactiveColor: p.border,
    },
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "line", lineStyle: { color: p.muted, width: 1, type: "dashed" } },
      // The tooltip is HTML, so it can be frosted glass like the rest of the
      // app. The glass goes on a wrapper inside the content rather than on
      // ECharts' own element (via `className`): that element exists, unstyled
      // and not yet hidden, before the first hover, and would show empty.
      backgroundColor: "transparent",
      borderWidth: 0,
      padding: 0,
      extraCssText: "box-shadow: none;",
      formatter: (params) => {
        const list = (Array.isArray(params) ? params : [params]) as {
          seriesName: string;
          value: number;
          dataIndex: number;
          color: unknown;
          seriesIndex: number;
        }[];
        const day = days[list[0]?.dataIndex ?? 0];
        if (!day) return "";
        const date = parseDay(day.day).toLocaleDateString(undefined, {
          weekday: "long",
          month: "short",
          day: "numeric",
        });
        const total = KINDS.reduce((a, k) => a + day[k.key], 0);
        // Top of the stack first, matching what's under the pointer.
        const rows = [...KINDS]
          .reverse()
          .map(
            (k) =>
              `<div class="${styles.tipRow}"${day[k.key] === 0 ? ` data-zero` : ""}>` +
              `<span class="${styles.tipSwatch}" style="background:${rgba(p.series[k.key], 1)}"></span>` +
              `<span>${k.label}</span><span class="${styles.tipValue}">${day[k.key]}</span></div>`,
          )
          .join("");
        return (
          `<div class="${styles.tooltip}">` +
          `<div class="${styles.tipHead}"><span>${date}</span><span class="${styles.tipValue}">${total}</span></div>` +
          rows +
          `</div>`
        );
      },
    },
    xAxis: {
      type: "category",
      boundaryGap: false,
      data: days.map((d) => dayLabel(d.day, range)),
      axisLine: { lineStyle: { color: p.border } },
      axisTick: { show: false },
      axisLabel: { color: p.muted, fontSize: 11, hideOverlap: true, margin: 12 },
    },
    yAxis: {
      type: "value",
      minInterval: 1,
      splitNumber: 4,
      axisLabel: { color: p.muted, fontSize: 11 },
      splitLine: { lineStyle: { color: p.border, type: "dashed" } },
    },
    series: KINDS.map(
      (k): LineSeriesOption => ({
        name: k.label,
        type: "line",
        stack: "activity",
        smooth: 0.35,
        showSymbol: false,
        symbol: "circle",
        symbolSize: 8,
        lineStyle: { width: 2, color: rgba(p.series[k.key], 1) },
        itemStyle: { color: rgba(p.series[k.key], 1) },
        areaStyle: {
          color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
            { offset: 0, color: rgba(p.series[k.key], 0.5) },
            { offset: 1, color: rgba(p.series[k.key], 0.04) },
          ]),
        },
        emphasis: { focus: "series" },
        data: days.map((d) => d[k.key]),
      }),
    ),
  };
}

/**
 * The workspace's activity per day, stacked by kind: items created, started
 * and completed, comments, and everything else. Follows live changes.
 */
export function ActivityChart({ workspaceId }: { workspaceId: string }) {
  const [range, setRange] = useState<Range>("7");
  const [days, setDays] = useState<DayStats[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [palette, setPalette] = useState<Palette | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const chartRef = useRef<echarts.ECharts | null>(null);

  const load = useCallback(async () => {
    try {
      const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
      const q = new URLSearchParams({ days: range, tz });
      setDays(await api.get<DayStats[]>(`/workspaces/${workspaceId}/activity/stats?${q}`));
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [workspaceId, range]);

  useEffect(() => {
    load();
  }, [load]);

  // Every recorded change counts, so any activity entry refreshes the chart.
  useLiveRefresh((e) => e.id !== null, load, { delay: 600 });

  // Create the chart once, keep it sized to its box, and re-read the theme's
  // colours whenever light/dark or the palette changes.
  useEffect(() => {
    const box = boxRef.current;
    if (!box) return;
    const chart = echarts.init(box, undefined, { renderer: "canvas" });
    chartRef.current = chart;
    const refreshPalette = () => setPalette(readPalette(box));
    refreshPalette();

    const resize = new ResizeObserver(() => chart.resize());
    resize.observe(box);
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    // The theme attribute changes before the new styles apply; wait a frame.
    const onTheme = () => requestAnimationFrame(refreshPalette);
    scheme.addEventListener("change", onTheme);
    window.addEventListener(THEME_CHANGE_EVENT, onTheme);
    return () => {
      resize.disconnect();
      scheme.removeEventListener("change", onTheme);
      window.removeEventListener(THEME_CHANGE_EVENT, onTheme);
      chart.dispose();
      chartRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!chartRef.current || !palette || !days) return;
    const animate = !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    chartRef.current.setOption(buildOption(days, range, palette, animate), { notMerge: true });
  }, [days, range, palette]);

  const total = useMemo(() => days?.reduce((a, d) => a + KINDS.reduce((s, k) => s + d[k.key], 0), 0) ?? 0, [days]);
  const phrase = RANGES.find((r) => r.value === range)!.phrase;

  return (
    <section className={styles.section} aria-label="Activity">
      <PageHeader
        title="Activity"
        level={2}
        description={days === null ? "Loading…" : `${total.toLocaleString()} ${total === 1 ? "change" : "changes"} in ${phrase}`}
        actions={
          <SegmentedControl
            label="Time range"
            size="sm"
            options={RANGES.map(({ value, label }) => ({ value, label }))}
            value={range}
            onChange={setRange}
          />
        }
      />
      {error && <Alert tone="danger">{error}</Alert>}
      <div className={styles.panel}>
        <div ref={boxRef} className={styles.canvas} aria-hidden="true" />
        {days !== null && total === 0 && <p className={styles.empty}>No activity in {phrase}</p>}
      </div>
      {days && (
        <div className="methods-sr-only">
          <table aria-label={`Activity per day in ${phrase}`}>
          <thead>
            <tr>
              <th scope="col">Day</th>
              {KINDS.map((k) => (
                <th key={k.key} scope="col">
                  {k.label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {days.map((d) => (
              <tr key={d.day}>
                <th scope="row">{parseDay(d.day).toLocaleDateString(undefined, { dateStyle: "medium" })}</th>
                {KINDS.map((k) => (
                  <td key={k.key}>{d[k.key]}</td>
                ))}
              </tr>
            ))}
          </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
