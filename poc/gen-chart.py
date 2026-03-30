#!/usr/bin/env python3
import json
import sys
from pathlib import Path


def percentile(sorted_vals, p):
    if not sorted_vals:
        return 0
    n = len(sorted_vals)
    idx = (p / 100) * (n - 1)
    lo = int(idx)
    hi = lo + 1
    if hi >= n:
        return sorted_vals[-1]
    frac = idx - lo
    return sorted_vals[lo] + frac * (sorted_vals[hi] - sorted_vals[lo])


def bucket_key(t):
    return int(t)


def process_records(records):
    req_buckets = {}
    sys_records = []

    for r in records:
        if r["type"] == "req":
            k = bucket_key(r["t"])
            if k not in req_buckets:
                req_buckets[k] = []
            req_buckets[k].append(r)
        elif r["type"] == "sys":
            sys_records.append(r)

    return req_buckets, sys_records


def build_latency_data(req_buckets):
    times, p50s, p95s, p99s = [], [], [], []
    for k in sorted(req_buckets):
        reqs = req_buckets[k]
        vals = sorted(r["total_us"] / 1000 for r in reqs)
        times.append(k)
        p50s.append(round(percentile(vals, 50), 3))
        p95s.append(round(percentile(vals, 95), 3))
        p99s.append(round(percentile(vals, 99), 3))
    return times, p50s, p95s, p99s


def build_phase_data(req_buckets):
    times, routes, forwards, relays = [], [], [], []
    for k in sorted(req_buckets):
        reqs = req_buckets[k]
        n = len(reqs)
        times.append(k)
        routes.append(round(sum(r["route_us"] for r in reqs) / n / 1000, 3))
        forwards.append(round(sum(r["forward_us"] for r in reqs) / n / 1000, 3))
        relays.append(round(sum(r["relay_us"] for r in reqs) / n / 1000, 3))
    return times, routes, forwards, relays


def build_throughput_data(req_buckets):
    times, counts = [], []
    for k in sorted(req_buckets):
        times.append(k)
        counts.append(len(req_buckets[k]))
    return times, counts


def build_error_data(req_buckets):
    times, errors = [], []
    for k in sorted(req_buckets):
        times.append(k)
        errors.append(sum(1 for r in req_buckets[k] if r.get("error")))
    return times, errors


def build_sys_data(sys_records):
    times = [r["t"] for r in sys_records]
    rss_mb = [round(r["rss"] / 1048576, 2) for r in sys_records]
    cpu = [r["cpu"] for r in sys_records]
    active = [r["active"] for r in sys_records]
    upstreams = [r["upstreams"] for r in sys_records]
    return times, rss_mb, cpu, active, upstreams


def render_html(lat_times, p50s, p95s, p99s,
                phase_times, routes, forwards, relays,
                tput_times, tput_counts,
                err_times, err_counts,
                sys_times, rss_mb, cpu, active, upstreams):
    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Railscale Benchmark</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #0d1117; color: #c9d1d9; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; padding: 24px; }}
  h1 {{ font-size: 1.5rem; margin-bottom: 24px; color: #e6edf3; }}
  .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }}
  .chart-box {{ background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 16px; }}
  .chart-box h2 {{ font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.08em; color: #8b949e; margin-bottom: 12px; }}
  canvas {{ width: 100% !important; }}
  .full {{ grid-column: 1 / -1; }}
</style>
</head>
<body>
<h1>Railscale Benchmark</h1>
<div class="grid">
  <div class="chart-box">
    <h2>Latency Over Time</h2>
    <canvas id="latency"></canvas>
  </div>
  <div class="chart-box">
    <h2>Phase Breakdown</h2>
    <canvas id="phase"></canvas>
  </div>
  <div class="chart-box">
    <h2>Throughput</h2>
    <canvas id="throughput"></canvas>
  </div>
  <div class="chart-box">
    <h2>Error Rate</h2>
    <canvas id="errors"></canvas>
  </div>
  <div class="chart-box full">
    <h2>System Resources</h2>
    <canvas id="system"></canvas>
  </div>
</div>
<script>
const defaults = {{
  pointRadius: 0,
  tension: 0.3,
  borderWidth: 2,
}};

const gridColor = "rgba(48,54,61,0.8)";
const tickColor = "#8b949e";

function baseScales(yLabel) {{
  return {{
    x: {{
      ticks: {{ color: tickColor }},
      grid: {{ color: gridColor }},
      title: {{ display: true, text: "Time (s)", color: tickColor }},
    }},
    y: {{
      ticks: {{ color: tickColor }},
      grid: {{ color: gridColor }},
      title: {{ display: true, text: yLabel, color: tickColor }},
    }},
  }};
}}

function baseLegend() {{
  return {{ labels: {{ color: "#c9d1d9", boxWidth: 12 }} }};
}}

new Chart(document.getElementById("latency"), {{
  type: "line",
  data: {{
    labels: {json.dumps(lat_times)},
    datasets: [
      {{ ...defaults, label: "p50", data: {json.dumps(p50s)}, borderColor: "#3fb950", backgroundColor: "transparent" }},
      {{ ...defaults, label: "p95", data: {json.dumps(p95s)}, borderColor: "#d29922", backgroundColor: "transparent" }},
      {{ ...defaults, label: "p99", data: {json.dumps(p99s)}, borderColor: "#f85149", backgroundColor: "transparent" }},
    ],
  }},
  options: {{
    animation: false,
    plugins: {{ legend: baseLegend() }},
    scales: baseScales("Latency (ms)"),
  }},
}});

new Chart(document.getElementById("phase"), {{
  type: "bar",
  data: {{
    labels: {json.dumps(phase_times)},
    datasets: [
      {{ label: "Route", data: {json.dumps(routes)}, backgroundColor: "#8957e5", stack: "s" }},
      {{ label: "Forward", data: {json.dumps(forwards)}, backgroundColor: "#58a6ff", stack: "s" }},
      {{ label: "Relay", data: {json.dumps(relays)}, backgroundColor: "#3fb950", stack: "s" }},
    ],
  }},
  options: {{
    animation: false,
    plugins: {{ legend: baseLegend() }},
    scales: {{ ...baseScales("Avg latency (ms)"), x: {{ ...baseScales("Avg latency (ms)").x, stacked: true }}, y: {{ ...baseScales("Avg latency (ms)").y, stacked: true }} }},
  }},
}});

new Chart(document.getElementById("throughput"), {{
  type: "line",
  data: {{
    labels: {json.dumps(tput_times)},
    datasets: [
      {{ ...defaults, label: "req/sec", data: {json.dumps(tput_counts)}, borderColor: "#58a6ff", backgroundColor: "transparent" }},
    ],
  }},
  options: {{
    animation: false,
    plugins: {{ legend: baseLegend() }},
    scales: baseScales("Requests / sec"),
  }},
}});

new Chart(document.getElementById("errors"), {{
  type: "line",
  data: {{
    labels: {json.dumps(err_times)},
    datasets: [
      {{ ...defaults, label: "errors/sec", data: {json.dumps(err_counts)}, borderColor: "#f85149", backgroundColor: "transparent" }},
    ],
  }},
  options: {{
    animation: false,
    plugins: {{ legend: baseLegend() }},
    scales: baseScales("Errors / sec"),
  }},
}});

new Chart(document.getElementById("system"), {{
  type: "line",
  data: {{
    labels: {json.dumps(sys_times)},
    datasets: [
      {{ ...defaults, label: "RSS (MB)", data: {json.dumps(rss_mb)}, borderColor: "#f0883e", backgroundColor: "transparent", yAxisID: "yLeft" }},
      {{ ...defaults, label: "CPU (%)", data: {json.dumps(cpu)}, borderColor: "#d29922", backgroundColor: "transparent", yAxisID: "yLeft" }},
      {{ ...defaults, label: "Active conns", data: {json.dumps(active)}, borderColor: "#58a6ff", backgroundColor: "transparent", yAxisID: "yRight" }},
      {{ ...defaults, label: "Upstreams", data: {json.dumps(upstreams)}, borderColor: "#3fb950", backgroundColor: "transparent", yAxisID: "yRight" }},
    ],
  }},
  options: {{
    animation: false,
    plugins: {{ legend: baseLegend() }},
    scales: {{
      x: {{ ticks: {{ color: tickColor }}, grid: {{ color: gridColor }}, title: {{ display: true, text: "Time (s)", color: tickColor }} }},
      yLeft: {{ type: "linear", position: "left", ticks: {{ color: tickColor }}, grid: {{ color: gridColor }}, title: {{ display: true, text: "RSS (MB) / CPU (%)", color: tickColor }} }},
      yRight: {{ type: "linear", position: "right", ticks: {{ color: tickColor }}, grid: {{ drawOnChartArea: false }}, title: {{ display: true, text: "Connections", color: tickColor }} }},
    }},
  }},
}});
</script>
</body>
</html>
"""


def main():
    if len(sys.argv) < 2:
        print("Usage: gen-chart.py <recording.jsonl>", file=sys.stderr)
        sys.exit(1)

    input_path = Path(sys.argv[1])
    records = []
    with input_path.open() as f:
        for line in f:
            line = line.strip()
            if line:
                records.append(json.loads(line))

    req_buckets, sys_records = process_records(records)

    if not req_buckets:
        print("Error: no request records found", file=sys.stderr)
        sys.exit(1)

    lat_times, p50s, p95s, p99s = build_latency_data(req_buckets)
    phase_times, routes, forwards, relays = build_phase_data(req_buckets)
    tput_times, tput_counts = build_throughput_data(req_buckets)
    err_times, err_counts = build_error_data(req_buckets)
    sys_times, rss_mb, cpu, active, upstreams = build_sys_data(sys_records)

    html = render_html(
        lat_times, p50s, p95s, p99s,
        phase_times, routes, forwards, relays,
        tput_times, tput_counts,
        err_times, err_counts,
        sys_times, rss_mb, cpu, active, upstreams,
    )

    output_path = input_path.with_suffix(".html")
    output_path.write_text(html)
    print(f"Chart written to {output_path}")


if __name__ == "__main__":
    main()
