-- CreateEnum
CREATE TYPE "subscription_status" AS ENUM ('ACTIVE', 'CANCELLED', 'PAST_DUE', 'UNPAID');

-- CreateTable
CREATE TABLE "email_training" (
    "id" SERIAL NOT NULL,
    "user_email" VARCHAR NOT NULL,
    "email_id" VARCHAR NOT NULL,
    "from" VARCHAR NOT NULL,
    "subject" VARCHAR NOT NULL,
    "body" TEXT NOT NULL,
    "ai_answer" VARCHAR NOT NULL,
    "confidence" REAL NOT NULL,
    "heuristics_used" BOOLEAN NOT NULL DEFAULT false,

    CONSTRAINT "email_training_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "inbox_settings" (
    "id" SERIAL NOT NULL,
    "user_id" INTEGER NOT NULL,
    "category" VARCHAR NOT NULL,
    "skip_inbox" BOOLEAN NOT NULL,
    "mark_spam" BOOLEAN NOT NULL,

    CONSTRAINT "inbox_settings_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "processed_daily_summary" (
    "id" SERIAL NOT NULL,
    "user_id" INTEGER NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "processed_daily_summary_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "processed_email" (
    "id" VARCHAR NOT NULL,
    "user_id" INTEGER NOT NULL,
    "processed_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "labels_applied" TEXT[],
    "labels_removed" TEXT[],
    "ai_answer" VARCHAR NOT NULL,

    CONSTRAINT "processed_email_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "user_account_access" (
    "id" SERIAL NOT NULL,
    "access_token" VARCHAR NOT NULL,
    "refresh_token" VARCHAR NOT NULL,
    "expires_at" TIMESTAMPTZ(6) NOT NULL,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "last_sync" TIMESTAMPTZ(6),
    "user_email" VARCHAR NOT NULL,

    CONSTRAINT "user_account_access_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "user_settings" (
    "id" SERIAL NOT NULL,
    "daily_summary_enabled" BOOLEAN NOT NULL DEFAULT true,
    "daily_summary_time" VARCHAR NOT NULL DEFAULT '06:00',
    "user_time_zone_offset" VARCHAR NOT NULL DEFAULT '-08',
    "user_email" VARCHAR NOT NULL,

    CONSTRAINT "user_settings_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "user_token_usage_stats" (
    "id" SERIAL NOT NULL,
    "date" DATE NOT NULL DEFAULT CURRENT_DATE,
    "tokens_consumed" BIGINT NOT NULL DEFAULT 0,
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "user_email" VARCHAR NOT NULL,

    CONSTRAINT "user_token_usage_stats_pkey" PRIMARY KEY ("id")
);

-- CreateTable
CREATE TABLE "user" (
    "id" SERIAL NOT NULL,
    "email" VARCHAR NOT NULL,
    "subscription_status" "subscription_status" NOT NULL DEFAULT 'UNPAID',
    "last_successful_payment_at" TIMESTAMPTZ(6),
    "last_payment_attempt_at" TIMESTAMPTZ(6),
    "created_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "user_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "email_training_email_id_key" ON "email_training"("email_id");

-- CreateIndex
CREATE INDEX "inbox_settings_user_id_idx" ON "inbox_settings"("user_id");

-- CreateIndex
CREATE UNIQUE INDEX "inbox_settings_category_user_id_key" ON "inbox_settings"("category", "user_id");

-- CreateIndex
CREATE INDEX "processed_daily_summary_user_id_idx" ON "processed_daily_summary"("user_id");

-- CreateIndex
CREATE INDEX "processed_email_user_id_idx" ON "processed_email"("user_id");

-- CreateIndex
CREATE UNIQUE INDEX "user_account_access_user_email_key" ON "user_account_access"("user_email");

-- CreateIndex
CREATE UNIQUE INDEX "user_settings_user_email_key" ON "user_settings"("user_email");

-- CreateIndex
CREATE UNIQUE INDEX "user_token_usage_stats_user_email_key" ON "user_token_usage_stats"("user_email");

-- CreateIndex
CREATE INDEX "user_token_usage_stats_date_idx" ON "user_token_usage_stats"("date");

-- CreateIndex
CREATE UNIQUE INDEX "user_token_usage_stats_date_user_email_key" ON "user_token_usage_stats"("date", "user_email");

-- CreateIndex
CREATE UNIQUE INDEX "user_email_key" ON "user"("email");

-- AddForeignKey
ALTER TABLE "inbox_settings" ADD CONSTRAINT "inbox_settings_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "processed_daily_summary" ADD CONSTRAINT "processed_daily_summary_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE NO ACTION;

-- AddForeignKey
ALTER TABLE "processed_email" ADD CONSTRAINT "processed_email_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE NO ACTION;

-- AddForeignKey
ALTER TABLE "user_account_access" ADD CONSTRAINT "user_account_access_user_email_fkey" FOREIGN KEY ("user_email") REFERENCES "user"("email") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "user_settings" ADD CONSTRAINT "user_settings_user_email_fkey" FOREIGN KEY ("user_email") REFERENCES "user"("email") ON DELETE RESTRICT ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "user_token_usage_stats" ADD CONSTRAINT "user_token_usage_stats_user_email_fkey" FOREIGN KEY ("user_email") REFERENCES "user"("email") ON DELETE RESTRICT ON UPDATE CASCADE;
