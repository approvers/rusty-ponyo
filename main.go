package main

import (
	"log"
	"os"

	"github.com/approvers/rusty-ponyo/bot/alias"
	"github.com/approvers/rusty-ponyo/client/discord"
	"github.com/joho/godotenv"
)

func main() {
	godotenv.Load()
	logger := log.New(os.Stdout, "", log.LstdFlags)

	d := discord.NewDiscordClient(logger)
	d.AddBot(alias.NewMessageAliasBot())

	token := os.Getenv("DISCORD_TOKEN")
	if token == "" {
		logger.Panicln("Failed to get DISCORD_TOKEN")
	}

	error := d.Start("")

	if error != nil {
		logger.Panicln(error)
	}
}
