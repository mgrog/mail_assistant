-- DropForeignKey
ALTER TABLE "user_settings" DROP CONSTRAINT "user_settings_user_email_fkey";

-- DropIndex
DROP INDEX "user_token_usage_stats_user_email_key";

-- AddForeignKey
ALTER TABLE "user_settings" ADD CONSTRAINT "user_settings_user_email_fkey" FOREIGN KEY ("user_email") REFERENCES "user"("email") ON DELETE CASCADE ON UPDATE NO ACTION;
