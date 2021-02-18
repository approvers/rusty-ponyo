package main

import (
	"github.com/approvers/rusty-ponyo/bot/alias"
	"github.com/approvers/rusty-ponyo/client/discord"
	"log"
	"os"
)

func main() {
	logger := log.New(os.Stdout, "", log.LstdFlags)

	d := discord.NewDiscordClient(logger)

	d.AddBot(alias.NewMessageAliasBot())

	error := d.Start("")

	if error != nil {
		logger.Panicln(error)
	}
}
