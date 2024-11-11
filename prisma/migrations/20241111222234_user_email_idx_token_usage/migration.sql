-- AlterTable
ALTER TABLE "user_token_usage_stats" ADD COLUMN     "month" INTEGER NOT NULL DEFAULT EXTRACT(MONTH FROM CURRENT_DATE),
ADD COLUMN     "year" INTEGER NOT NULL DEFAULT EXTRACT(YEAR FROM CURRENT_DATE);

-- CreateIndex
CREATE INDEX "user_token_usage_stats_user_email_idx" ON "user_token_usage_stats"("user_email");
