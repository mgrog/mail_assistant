-- DropForeignKey
ALTER TABLE "processed_daily_summary" DROP CONSTRAINT "processed_daily_summary_user_id_fkey";

-- DropForeignKey
ALTER TABLE "processed_email" DROP CONSTRAINT "processed_email_user_id_fkey";

-- DropForeignKey
ALTER TABLE "user_account_access" DROP CONSTRAINT "user_account_access_user_email_fkey";

-- DropForeignKey
ALTER TABLE "user_settings" DROP CONSTRAINT "user_settings_user_email_fkey";

-- DropForeignKey
ALTER TABLE "user_token_usage_stats" DROP CONSTRAINT "user_token_usage_stats_user_email_fkey";

-- AddForeignKey
ALTER TABLE "processed_daily_summary" ADD CONSTRAINT "processed_daily_summary_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "processed_email" ADD CONSTRAINT "processed_email_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "user_account_access" ADD CONSTRAINT "user_account_access_user_email_fkey" FOREIGN KEY ("user_email") REFERENCES "user"("email") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "user_settings" ADD CONSTRAINT "user_settings_user_email_fkey" FOREIGN KEY ("user_email") REFERENCES "user"("email") ON DELETE CASCADE ON UPDATE CASCADE;

-- AddForeignKey
ALTER TABLE "user_token_usage_stats" ADD CONSTRAINT "user_token_usage_stats_user_email_fkey" FOREIGN KEY ("user_email") REFERENCES "user"("email") ON DELETE CASCADE ON UPDATE CASCADE;
