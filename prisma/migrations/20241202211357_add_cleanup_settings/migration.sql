/*
  Warnings:

  - You are about to drop the `inbox_settings` table. If the table is not empty, all the data it contains will be lost.
  - You are about to drop the `user_settings` table. If the table is not empty, all the data it contains will be lost.
  - Added the required column `category` to the `processed_email` table without a default value. This is not possible if the table is not empty.

*/
-- CreateEnum
CREATE TYPE "cleanup_action" AS ENUM ('DELETE', 'ARCHIVE', 'NOTHING');

-- DropForeignKey
ALTER TABLE "inbox_settings" DROP CONSTRAINT "inbox_settings_user_id_fkey";

-- DropForeignKey
ALTER TABLE "user_settings" DROP CONSTRAINT "user_settings_user_email_fkey";


DO $$
BEGIN
    -- AlterTable
    ALTER TABLE "processed_email" ADD COLUMN     "category" VARCHAR;

    -- Step 3: Update existing rows
    UPDATE "processed_email"
    SET "category" = "labels_applied"[array_upper("labels_applied", 1)];

    -- Step 5: Set the `category` column to NOT NULL
    ALTER TABLE "processed_email"
    ALTER COLUMN "category" SET NOT NULL;
END $$;

-- DropTable
DROP TABLE "inbox_settings";

-- DropTable
DROP TABLE "user_settings";

-- CreateTable
CREATE TABLE "cleanup_settings" (
    "id" SERIAL NOT NULL,
    "user_id" INTEGER NOT NULL,
    "category" VARCHAR NOT NULL,
    "days_old" INTEGER NOT NULL,
    "action" "cleanup_action" NOT NULL,

    CONSTRAINT "cleanup_settings_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "cleanup_settings_user_id_idx" ON "cleanup_settings"("user_id");

-- CreateIndex
CREATE UNIQUE INDEX "cleanup_settings_category_user_id_key" ON "cleanup_settings"("category", "user_id");

-- AddForeignKey
ALTER TABLE "cleanup_settings" ADD CONSTRAINT "cleanup_settings_user_id_fkey" FOREIGN KEY ("user_id") REFERENCES "user"("id") ON DELETE CASCADE ON UPDATE CASCADE;
