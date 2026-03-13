library(ggplot2)
library(dplyr)
library(tidyr)
library(scales)
library(patchwork)
library(ggtext)
library(broom) # For extracting regression coefficients

# --- Configuration & Theme ---
# Using a more robust palette for scientific publishing
pal <- c(
  "blue" = "#0072B2", 
  "orange" = "#D55E00", 
  "green" = "#009E73", 
  "red" = "#CC79A7",
  "grey" = "#999999"
)

theme_research <- function() {
  theme_minimal(base_family = "sans", base_size = 11) +
    theme(
      plot.title = element_markdown(face = "bold", size = 14),
      plot.subtitle = element_text(size = 11, color = "grey30"),
      plot.caption = element_text(size = 8, face = "italic"),
      panel.grid.minor = element_blank(),
      panel.border = element_rect(fill = NA, color = "grey80"),
      strip.background = element_rect(fill = "grey95", color = "grey80"),
      strip.text = element_text(face = "bold"),
      legend.position = "top",
      legend.title = element_text(size = 10)
    )
}

# --- Data Preparation ---
df <- read.csv("riblt_benchmarks.csv") %>%
  mutate(
    bandwidth_per_diff = bandwidth_bytes / diff_size,
    total_time_ms = (encode_time_us + decode_time_us) / 1000,
    overhead_ratio = bandwidth_bytes / (diff_size * 64) # Assuming 64-byte elements
  )

# --- 1. Scaling Analysis (Latency & Complexity) ---
# We fit a linear model in log-log space to verify O(n)
fit_data <- df %>% filter(scenario == "scale_diff")
lm_encode <- lm(log(encode_time_us) ~ log(diff_size), data = fit_data)
slope <- round(coef(lm_encode)[2], 2)

p_latency <- fit_data %>%
  pivot_longer(cols = c(encode_time_us, decode_time_us), names_to = "op", values_to = "us") %>%
  mutate(op = ifelse(op == "encode_time_us", "Encoding", "Decoding")) %>%
  ggplot(aes(x = diff_size, y = us / 1000, color = op)) +
  geom_smooth(method = "lm", formula = y ~ x, linetype = "dashed", alpha = 0.1, linewidth = 0.5) +
  stat_summary(fun = mean, geom = "line", linewidth = 1) +
  stat_summary(fun.data = mean_cl_boot, geom = "ribbon", alpha = 0.2, color = NA) +
  scale_x_log10(labels = label_log()) +
  scale_y_log10(labels = label_number(suffix = " ms")) +
  scale_color_manual(values = c("Encoding" = pal["blue"], "Decoding" = pal["orange"])) +
  labs(
    title = "Latency Scaling: **O(Δ)** Verification",
    subtitle = paste0("Empirical slope: β ≈ ", slope, " (Expected 1.0)"),
    x = "Symmetric Difference Size |Δ|",
    y = "Time (ms)",
    color = "Operation"
  ) +
  theme_research()

# --- 2. Phase Transition (Success Rate) ---
# IBLT decoding exhibits a sharp threshold depending on the cell-to-difference ratio
p_success <- df %>%
  filter(scenario == "varying_overhead") %>%
  group_by(ratio) %>%
  summarise(
    rate = mean(success == "true"),
    se = sqrt(rate * (1 - rate) / n()),
    .groups = "drop"
  ) %>%
  ggplot(aes(x = ratio, y = rate)) +
  geom_vline(xintercept = 1.35, linetype = "dotted", color = pal["red"]) +
  annotate("text", x = 1.37, y = 0.2, label = "Peeling Threshold (~1.35x)", angle = 90, size = 3, color = pal["red"]) +
  geom_line(color = pal["green"], linewidth = 1.2) +
  geom_point(color = pal["green"], size = 3) +
  geom_errorbar(aes(ymin = rate - se, ymax = rate + se), width = 0.05) +
  scale_y_continuous(labels = label_percent(), limits = c(0, 1.05)) +
  labs(
    title = "Decoding Phase Transition",
    subtitle = "Success rate vs. Cell/Difference Ratio (m/k)",
    x = "Overhead Ratio (k=1000)",
    y = "Success Probability"
  ) +
  theme_research()

# --- 3. Bandwidth Efficiency ---
p_bandwidth <- df %>%
  filter(scenario == "scale_diff") %>%
  ggplot(aes(x = diff_size, y = bandwidth_per_diff)) +
  geom_hline(yintercept = 80, linetype = "dashed", color = "grey50") + # Theoretical cell size
  stat_summary(fun = mean, geom = "area", fill = pal["blue"], alpha = 0.1) +
  stat_summary(fun = mean, geom = "line", color = pal["blue"], linewidth = 1) +
  scale_x_log10(labels = label_log()) +
  scale_y_continuous(limits = c(0, NA), labels = label_number(suffix = " B/Δ")) +
  labs(
    title = "Bandwidth Efficiency",
    subtitle = "Amortized cost per difference element",
    x = "|Δ|",
    y = "Bytes per Δ"
  ) +
  theme_research()

# --- Performance Summary ---
summary_stats <- df %>%
  filter(scenario == "scale_diff") %>%
  summarise(
    avg_encode_us_per_element = mean(encode_time_us / diff_size, na.rm = TRUE),
    avg_decode_us_per_element = mean(decode_time_us / diff_size, na.rm = TRUE),
    avg_throughput_eps = mean(diff_size / ((encode_time_us + decode_time_us) / 1e6), na.rm = TRUE),
    avg_bandwidth_byte_per_diff = mean(bandwidth_per_diff, na.rm = TRUE)
  )

# --- Timestamp for Versioning ---
ts <- format(Sys.time(), "%Y%m%d_%H%M%S")

# --- Save Plots ---
# Individual plots for Markdown embedding
p1_name <- sprintf("latency_scaling_%s.png", ts)
p2_name <- sprintf("phase_transition_%s.png", ts)
p3_name <- sprintf("bandwidth_efficiency_%s.png", ts)
dashboard_name <- sprintf("riblt_performance_analysis_%s.png", ts)

# ggsave(p1_name, p_latency, width = 7, height = 5, dpi = 300)
# ggsave(p2_name, p_success, width = 7, height = 5, dpi = 300)
# ggsave(p3_name, p_bandwidth, width = 7, height = 5, dpi = 300)

# Composite dashboard as PNG
layout <- (p_latency | p_success) / p_bandwidth + 
  plot_layout(heights = c(1, 0.8)) +
  plot_annotation(
    title = "RIBLT Benchmarking Analysis",
    theme = theme(plot.title = element_text(size = 18, face = "bold", hjust = 0.5))
  )
ggsave(dashboard_name, layout, width = 12, height = 10, dpi = 300)

# --- Generate Markdown Report ---
report_md <- sprintf("riblt_performance_analysis_%s.md", ts)
cat(sprintf("# RIBLT Performance Analysis (%s)\n\n", format(Sys.time(), "%Y-%m-%d %H:%M:%S")), file = report_md)
cat("## Key Metrics\n\n", file = report_md, append = TRUE)
cat("| Metric | Value |\n", file = report_md, append = TRUE)
cat("| :--- | :--- |\n", file = report_md, append = TRUE)
cat(sprintf("| Avg Encode Time | %.2f μs/element |\n", summary_stats$avg_encode_us_per_element), file = report_md, append = TRUE)
cat(sprintf("| Avg Decode Time | %.2f μs/element |\n", summary_stats$avg_decode_us_per_element), file = report_md, append = TRUE)
cat(sprintf("| Avg Throughput | %s elements/sec |\n", format(round(summary_stats$avg_throughput_eps, 0), big.mark=",")), file = report_md, append = TRUE)
cat(sprintf("| Avg Bandwidth | %.1f bytes/element |\n", summary_stats$avg_bandwidth_byte_per_diff), file = report_md, append = TRUE)
cat("\n## Visualizations\n\n", file = report_md, append = TRUE)
cat(sprintf("### Full Dashboard\n![Full Dashboard](%s)\n", dashboard_name), file = report_md, append = TRUE)

# Also create/update a symlink or a copy for "latest" version
latest_report <- "riblt_performance_analysis.md"
file.copy(report_md, latest_report, overwrite = TRUE)

message(sprintf("Summary report and plots generated successfully: %s", report_md))