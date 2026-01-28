library(ggplot2)
library(dplyr)
library(tidyr)
library(scales)

# Read data
df <- read.csv("riblt_benchmarks.csv")

# Calculate derived metrics
df <- df %>%
  mutate(
    bandwidth_per_diff = bandwidth_bytes / diff_size,
    total_time_ms = (encode_time_us + decode_time_us) / 1000,
    throughput_diffs_per_sec = diff_size / (total_time_ms / 1000)
  )

# Theme setup
theme_pub <- theme_minimal() +
  theme(
    plot.title = element_text(size = 16, face = "bold"),
    axis.title = element_text(size = 12),
    legend.position = "bottom",
    panel.grid.minor = element_blank(),
    panel.border = element_rect(fill = NA, color = "grey80"),
    strip.background = element_rect(fill = "grey90"),
    strip.text = element_text(face = "bold")
  )

# 1. Bandwidth Efficiency vs Difference Size
p1 <- df %>%
  filter(scenario == "scale_diff") %>%
  group_by(diff_size) %>%
  summarise(
    mean_bw = mean(bandwidth_per_diff),
    sd_bw = sd(bandwidth_per_diff),
    .groups = "drop"
  ) %>%
  ggplot(aes(x = diff_size, y = mean_bw)) +
  geom_line(color = "#2c3e50", linewidth = 1.2) +
  geom_ribbon(aes(ymin = mean_bw - sd_bw, ymax = mean_bw + sd_bw), 
              alpha = 0.2, fill = "#2c3e50") +
  geom_point(size = 3, color = "#e74c3c") +
  scale_x_log10(labels = comma) +
  scale_y_continuous(labels = comma_format(suffix = " B/diff")) +
  labs(
    title = "Bandwidth Efficiency vs Set Difference Size",
    subtitle = "Fixed overhead ratio 1.5x, total set size 100k",
    x = "Symmetric Difference Size |A Δ B|",
    y = "Bandwidth per Difference Element",
    caption = "Lower is better. Error bands show ±1 SD over 10 trials"
  ) +
  theme_pub

ggsave("bandwidth_vs_diffsize.pdf", p1, width = 10, height = 6)

# 2. Success Rate vs Encoding Overhead
p2 <- df %>%
  filter(scenario == "varying_overhead") %>%
  group_by(ratio) %>%
  summarise(
    success_rate = mean(success),
    .groups = "drop"
  ) %>%
  ggplot(aes(x = ratio, y = success_rate)) +
  geom_line(color = "#27ae60", linewidth = 1.5) +
  geom_point(size = 4, color = "#27ae60") +
  geom_hline(yintercept = 1.0, linetype = "dashed", color = "red", alpha = 0.5) +
  scale_y_continuous(labels = percent, limits = c(0, 1)) +
  scale_x_continuous(breaks = unique(df$ratio)) +
  labs(
    title = "Decoding Success Rate vs Encoding Overhead",
    subtitle = "Fixed difference size = 1,000 elements",
    x = "Overhead Ratio (Blocks / Difference Size)",
    y = "Success Rate",
    caption = "Theoretical threshold typically around 1.35-1.5x for IBLT"
  ) +
  theme_pub

ggsave("success_rate_vs_overhead.pdf", p2, width = 10, height = 6)

# 3. Latency Breakdown by Component
p3 <- df %>%
  filter(scenario == "scale_diff") %>%
  select(diff_size, trial, encode_time_us, decode_time_us) %>%
  pivot_longer(
    cols = c(encode_time_us, decode_time_us),
    names_to = "operation",
    values_to = "time_us"
  ) %>%
  mutate(
    operation = ifelse(operation == "encode_time_us", "Encoding", "Decoding"),
    time_ms = time_us / 1000
  ) %>%
  group_by(diff_size, operation) %>%
  summarise(
    mean_time = mean(time_ms),
    sd_time = sd(time_ms),
    .groups = "drop"
  ) %>%
  ggplot(aes(x = diff_size, y = mean_time, color = operation, fill = operation)) +
  geom_line(linewidth = 1.2) +
  geom_ribbon(aes(ymin = mean_time - sd_time, ymax = mean_time + sd_time), 
              alpha = 0.2, color = NA) +
  geom_point(size = 3) +
  scale_x_log10(labels = comma) +
  scale_y_log10(labels = comma_format(suffix = " ms")) +
  scale_color_manual(values = c("Encoding" = "#3498db", "Decoding" = "#e67e22")) +
  scale_fill_manual(values = c("Encoding" = "#3498db", "Decoding" = "#e67e22")) +
  labs(
    title = "Latency Scaling with Difference Size",
    subtitle = "Encoding is O(n), Decoding is O(m) where m is IBLT size",
    x = "Difference Size",
    y = "Time (ms)",
    color = "Operation",
    fill = "Operation",
    caption = "Log-log scale shows linear scaling"
  ) +
  theme_pub

ggsave("latency_scaling.pdf", p3, width = 10, height = 6)

# 4. Memory Consumption Analysis
p4 <- df %>%
  filter(scenario == "scale_diff") %>%
  mutate(memory_mb = memory_bytes / 1e6) %>%
  group_by(diff_size) %>%
  summarise(
    mean_mem = mean(memory_mb),
    sd_mem = sd(memory_mb),
    .groups = "drop"
  ) %>%
  ggplot(aes(x = diff_size, y = mean_mem)) +
  geom_col(fill = "#9b59b6", alpha = 0.7, color = "black") +
  geom_errorbar(aes(ymin = mean_mem - sd_mem, ymax = mean_mem + sd_mem), 
                width = 0.2) +
  scale_x_log10(labels = comma) +
  scale_y_continuous(labels = comma_format(suffix = " MB")) +
  labs(
    title = "IBLT Memory Footprint",
    subtitle = "Memory required at transmitter (Alice) before network send",
    x = "Difference Size",
    y = "Memory (MB)",
    caption = "Linear growth ~72 bytes per block (40B data + 16B metadata) × 1.5 overhead"
  ) +
  theme_pub

ggsave("memory_footprint.pdf", p4, width = 10, height = 6)

# 5. Density vs Performance (Set size varies, diff constant)
p5 <- df %>%
  filter(scenario == "varying_density") %>%
  group_by(total_size) %>%
  summarise(
    mean_time = mean(total_time_ms),
    mean_bw = mean(bandwidth_per_diff),
    .groups = "drop"
  ) %>%
  pivot_longer(
    cols = c(mean_time, mean_bw),
    names_to = "metric",
    values_to = "value"
  ) %>%
  mutate(
    metric = ifelse(metric == "mean_time", "Latency (ms)", "Bandwidth (B/diff)"),
    density = diff_size[1] / total_size # diff_size is constant 1000 in this scenario
  ) %>%
  ggplot(aes(x = total_size, y = value, color = metric)) +
  geom_line(linewidth = 1.2) +
  geom_point(size = 3) +
  facet_wrap(~metric, scales = "free_y", ncol = 1) +
  scale_x_log10(labels = comma) +
  scale_color_manual(values = c("Bandwidth (B/diff)" = "#16a085", "Latency (ms)" = "#d35400")) +
  labs(
    title = "Performance vs Set Size (Sparse vs Dense Differences)",
    subtitle = "Constant difference size = 1,000",
    x = "Total Set Size (log scale)",
    y = NULL,
    caption = "Performance is largely independent of total set size (O(diff) complexity)"
  ) +
  theme_pub +
  theme(legend.position = "none")

ggsave("density_analysis.pdf", p5, width = 10, height = 8)

# Summary statistics table
summary_stats <- df %>%
  group_by(scenario) %>%
  summarise(
    n = n(),
    mean_success = mean(success),
    mean_encode_ms = mean(encode_time_us) / 1000,
    mean_decode_ms = mean(decode_time_us) / 1000,
    mean_bw_eff = mean(bandwidth_per_diff),
    .groups = "drop"
  )

print(summary_stats)
write.csv(summary_stats, "benchmark_summary.csv", row.names = FALSE)

cat("Plots generated:\n")
cat("1. bandwidth_vs_diffsize.pdf\n")
cat("2. success_rate_vs_overhead.pdf\n")
cat("3. latency_scaling.pdf\n")
cat("4. memory_footprint.pdf\n")
cat("5. density_analysis.pdf\n")