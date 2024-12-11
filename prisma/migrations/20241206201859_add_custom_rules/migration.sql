/*
  Warnings:

  - You are about to drop the `cleanup_settings` table. If the table is not empty, all the data it contains will be lost.
  - You are about to drop the `user_token_usage_stats` table. If the table is not empty, all the data it contains will be lost.

*/
-- DropForeignKey
ALTER TABLE "cleanup_settings" DROP CONSTRAINT "cleanup_settings_user_id_fkey";

-- DropForeignKey
ALTER TABLE "user_token_usage_stats" DROP CONSTRAINT "user_token_usage_stats_user_email_fkey";

-- DropTable
DROP TABLE "cleanup_settings";

-- DropTable
DROP TABLE "user_token_usage_stats";

-- CreateTable
CREATE TABLE "user_token_usage_stat" (
    "id" SERIAL NOT NULL,
    "date" DATE NOT NULL DEFAULT CURRENT_DATE,
    "month" INTEGER NOT NULL DEFAULT EXTRACT(MONTH FROM CURRENT_DATE),
    "year" INTEGER NOT NULL DEFAULT EXTRACT(YEAR FROM CURRENT_DATE),
    "tokens_consumed" BIGINT NOT NULL DEFAULT 0,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "user_email" VARCHAR NOT NULL,

    CONSTRAINT "user_token_usage_stat_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "default_email_rule_override" (
    "id" SERIAL NOT NULL,
    "user_id" INTEGER NOT NULL,
    "category" VARCHAR NOT NULL,
    "is_disabled" BOOLEAN NOT NULL DEFAULT false,
    "after_days_old" INTEGER NOT NULL DEFAULT 7,
    "cleanup_action" "cleanup_action" NOT NULL DEFAULT 'NOTHING',

    CONSTRAINT "default_email_rule_override_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "custom_email_rule" (
    "id" SERIAL NOT NULL,
    "user_id" INTEGER NOT NULL,
    "prompt_content" TEXT NOT NULL,
    "category" VARCHAR NOT NULL,
    "after_days_old" INTEGER NOT NULL DEFAULT 7,
    "cleanup_action" "cleanup_action" NOT NULL DEFAULT 'NOTHING',

    CONSTRAINT "custom_email_rule_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "user_token_usage_stat_date_idx" ON "user_token_usage_stat"("date");

-- CreateIndex
CREATE INDEX "user_token_usage_stat_user_email_idx" ON "user_token_usage_stat"("user_email");

-- CreateIndex
CREATE INDEX "user_token_usage_stat_user_email_month_year_idx" ON "user_token_usage_stat"("user_email", "month", "year");

-- CreateIndex
CREATE UNIQUE INDEX "user_token_usage_stat_date_user_email_key" ON "user_token_usage_stat"("date", "user_email");

-- CreateIndex
CREATE INDEX "default_email_rule_override_user_id_idx" ON "default_email_rule_override"("user_id");

-- CreateIndex
CREATE UNIQUE INDEX "default_email_rule_override_category_user_id_key" ON "default_email_rule_override"("category", "user_id");

-- CreateIndex
CREATE INDEX "custom_email_rule_user_id_idx" ON "custom_email_rule"("user_id");

-- AddForeignKey
ALTER TABLE "user_token_usage_stat" ADD CONSTRAINT "user_token_usage_stat_user_email_fkey" FOREIGN KEY ("user_email") REFERENCES "user"("email") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "default_email_rule_override" ADD CONSTRAINT "default_email_rule_override_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "custom_email_rule" ADD CONSTRAINT "custom_email_rule_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE CASCADE;
