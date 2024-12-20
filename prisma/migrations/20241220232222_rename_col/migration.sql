/*
  Warnings:

  - You are about to drop the column `is_valid` on the `user_account_access` table. All the data in the column will be lost.

*/
-- AlterTable
ALTER TABLE "user_account_access" DROP COLUMN "is_valid",
ADD COLUMN     "needs_reauthentication" BOOLEAN NOT NULL DEFAULT false;
