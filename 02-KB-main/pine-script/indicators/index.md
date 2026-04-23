---
title: pine-script — Indicators
parent: [[../index]]
last_updated: 2026-04-20
---

# pine-script — Indicators

Indicators render visualizations and fire alerts. They cannot place
orders — for that, see [[../strategy/index]].

## Declaration

```pine
//@version=6
indicator(
  title = "Example",
  shorttitle = "EX",
  overlay = true,           // share price pane; false = new pane below
  format = format.price,
  precision = 2,
  max_bars_back = 500,
  max_lines_count = 500,
  max_labels_count = 50,
  max_boxes_count = 50,
  max_polylines_count = 100,
  explicit_plot_zorder = false,
  timeframe = "",           // force higher TF; empty = chart TF
  timeframe_gaps = false
)
```

## Plotting primitives

| Call | What it does |
|---|---|
| `plot(series, title, color, linewidth, style, trackprice, histbase, offset, join, editable, show_last, display)` | Line/histogram/column plot. ★v6: `linestyle = solid/dashed/dotted/dashed_dotted` param |
| `plotshape(series, title, style, location, color, text, ...)` | Triangles, arrows, circles at bars where `series` is true |
| `plotchar(series, title, char, location, color, text, ...)` | Unicode character at bars |
| `plotbar(open, high, low, close, title, color)` | Re-render candles |
| `plotcandle(open, high, low, close, title, color, wickcolor, bordercolor)` | Same with explicit wick/border |
| `plotarrow(series, title, colorup, colordown, offset, minheight, maxheight)` | Proportional-height arrow |
| `hline(price, title, color, linestyle, linewidth)` | Horizontal reference line |
| `fill(hline1, hline2, color, title)` or `fill(plot1, plot2, color, title)` | Region fill |
| `bgcolor(color, offset, editable, title)` | Color chart background |
| `barcolor(color, offset)` | Color individual bars |

## Alerts

```pine
// alertcondition: shows up in the "Create Alert" dialog; does NOT fire
alertcondition(ta.crossover(close, ta.sma(close, 20)),
               title = "Cross above SMA20",
               message = "Close crossed above SMA20")

// alert: fires during script execution on the realtime bar
if ta.crossover(close, ta.sma(close, 20))
    alert("Cross above SMA20", alert.freq_once_per_bar_close)
```

Frequencies: `alert.freq_all` (every tick), `alert.freq_once_per_bar`
(first tick of bar), `alert.freq_once_per_bar_close` (when bar closes).

## Minimal indicator

```pine
//@version=6
indicator("RSI", shorttitle = "RSI", overlay = false)
length = input.int(14, minval = 1)
src    = input.source(close, "Source")
r = ta.rsi(src, length)
plot(r, color = color.purple)
hline(70, "Overbought", color = color.red)
hline(30, "Oversold",   color = color.green)
```

## See also

- [[../strategy/index]]
- [[../builtins/index]] — full `ta.*` list
- [[../pitfalls#ta-in-conditional-scope]]
- Canonical: `wiki.md` § Script Types
