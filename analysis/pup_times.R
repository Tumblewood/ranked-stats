library(dplyr)
library(ggplot2)
library(tidyr)

df <- read.csv("pup_times.csv", header = TRUE)

tw_theme <- theme_bw() +
  theme(text = element_text(family = "Helvetica Bold"),
        line = element_line(color = "#999999",
                            size = 0.1),
        plot.background = element_rect(fill = "#555555"),
        legend.position = "none",
        panel.background = element_blank(),
        panel.border = element_blank(),
        panel.grid.minor = element_line(size = 0.05),
        panel.grid.major.x = element_blank(),
        panel.grid.minor.x = element_blank(),
        axis.ticks = element_blank(),
        plot.title = element_text(hjust = 0.5,
                                  color = "#ffffff",
                                  size = 10),
        axis.title = element_text(size = 8,
                                  color = "#dddddd"),
        axis.text = element_text(family = "Helvetica Light",
                                 color = "#cccccc",
                                 size = 6),
        axis.text.x = element_text(hjust = 0.5,
                                   margin = margin(t = -4,
                                                   b = 2)))

faceted_theme <-
  theme(axis.text.x = element_text(margin = margin(t = -2,
                                                   b = 0)),
        axis.text.y = element_blank(),
        panel.grid.major.y = element_line(size = 0.05),
        panel.grid.minor.y = element_blank(),
        strip.background = element_blank(),
        strip.text = element_text(size = 10,
                                  color = "#dddddd"))

df$delay <- (df$time %% 3600) / 60
df$minute <- (df$time - (df$time %% 3600)) / 3600
df$minute[df$minute >= 8] <- "8+"
df$minute <- paste("Round", df$minute)
levels(df$pup_type) <- c("Juke Juice", "Rolling Bomb", "TagPro")

pup_delay_faceted <- ggplot(df %>% filter(delay <= 20), aes(x = delay)) +
  # geom_density(color = "#ff8888",
  #              fill = "#ffaaaa") +
  geom_histogram(binwidth = 0.5,
                 color = "#ff8888",
                 fill = "#ffaaaa") +
  facet_wrap(~minute, nrow = 2) +
  labs(title = "How much do powerups get delayed in successive pup rounds?",
       x = "Delay",
       y = "Relative frequency") +
  tw_theme +
  faceted_theme

ggsave("pup_delay_faceted.png", pup_delay_faceted,
       width = 1920 / 300, height = 1080 / 300, dpi = 300)

freq_by_pup <- ggplot(df %>% filter(time < 7200, delay <= 5),
       aes(x = delay, fill = pup_type)) +
  geom_density(color = "#dddddd",
               alpha = 0.5) +
  labs(title = "How early do different powerups get collected in the first pup round?",
       x = "Delay (seconds after powerups first spawn)") +
  tw_theme +
  theme(axis.title.y = element_blank(),
        legend.position = "right",
        legend.title = element_blank(),
        legend.background = element_blank(),
        legend.text = element_text(color = "#dddddd"))

ggsave("freq_by_pup.png", freq_by_pup,
       width = 1920 / 300, height = 1080 / 300, dpi = 300)

df %>%
  filter(time <= 7200) %>%
  group_by(pup_type) %>%
  summarize(avg_delay = mean(delay), count = n())

pup_types_by_player <- df %>%
  count(player, pup_type) %>%
  pivot_wider(names_from = pup_type, values_from = n, values_fill = 0) %>%
  mutate(
    total = rowSums(across(where(is.numeric))),
    # across(where(is.numeric) & !total, ~ .x - total/3, .names = "{.col}_diff"),
    # across(where(is.numeric) & !total, ~ (.x - total/3) / (total/3), .names = "{.col}_diffpct")
  ) %>%
  filter(total >= 50)

write.csv(pup_types_by_player, "pups_by_player.csv")

df %>%
  filter(minute <= "Round 7") %>%
  group_by(match_id, player) %>%
  summarize(count = n()) %>%
  arrange(desc(count))

length(unique(df$match_id))
