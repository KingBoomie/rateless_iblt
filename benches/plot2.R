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

# --- Composite Dashboard ---
# Using patchwork to combine into a single publication-ready figure
layout <- (p_latency | p_success) / p_bandwidth + 
  plot_layout(heights = c(1, 0.8)) +
  plot_annotation(
    title = "RIBLT Benchmarking Analysis",
    theme = theme(plot.title = element_text(size = 18, face = "bold", hjust = 0.5))
  )

ggsave("riblt_performance_analysis.pdf", layout, width = 12, height = 10, device = cairo_pdf)