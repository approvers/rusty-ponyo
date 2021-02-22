package model

import "time"

type MessageAlias struct {
	Key              string
	Message          string
	CreatorDiscordID string
	CreatedAt        time.Time
}
