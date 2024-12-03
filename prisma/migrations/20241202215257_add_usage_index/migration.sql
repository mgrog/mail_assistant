-- CreateIndex
CREATE INDEX "user_token_usage_stats_user_email_month_year_idx" ON "user_token_usage_stats"("user_email", "month", "year");
