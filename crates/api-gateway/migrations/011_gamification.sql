-- Phase 3a: XP, achievements, missions, notifications

CREATE TABLE user_xp (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    total_xp INT NOT NULL DEFAULT 0,
    level INT NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE user_streaks (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    current_streak INT NOT NULL DEFAULT 0,
    longest_streak INT NOT NULL DEFAULT 0,
    last_login_date DATE
);

CREATE TABLE achievements (
    id SERIAL PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    title_de TEXT NOT NULL,
    description_de TEXT NOT NULL,
    xp_reward INT NOT NULL DEFAULT 0,
    badge_slug TEXT NOT NULL
);

CREATE TABLE user_achievements (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    achievement_id INT NOT NULL REFERENCES achievements(id) ON DELETE CASCADE,
    unlocked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, achievement_id)
);

CREATE TABLE missions (
    id SERIAL PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    title_de TEXT NOT NULL,
    type TEXT NOT NULL CHECK (type IN ('daily', 'weekly', 'monthly')),
    target_count INT NOT NULL,
    reward_usdt NUMERIC(28, 18) NOT NULL DEFAULT 0,
    xp_reward INT NOT NULL DEFAULT 0
);

CREATE TABLE user_mission_progress (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mission_id INT NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    progress INT NOT NULL DEFAULT 0,
    completed_at TIMESTAMPTZ,
    claimed_at TIMESTAMPTZ,
    period_start DATE NOT NULL,
    PRIMARY KEY (user_id, mission_id, period_start)
);

CREATE TABLE notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    archived_at TIMESTAMPTZ
);

CREATE INDEX idx_notifications_user ON notifications(user_id, created_at DESC);
CREATE INDEX idx_user_mission_progress_user ON user_mission_progress(user_id, period_start);

INSERT INTO achievements (slug, title_de, description_de, xp_reward, badge_slug) VALUES
    ('first_ad', 'Erstes Video', 'Dein erstes Werbevideo geschaut', 25, 'play'),
    ('ads_10', '10 Videos', '10 Werbevideos geschaut', 50, 'play-10'),
    ('ads_100', '100 Videos', '100 Werbevideos geschaut', 150, 'play-100'),
    ('ads_500', '500 Videos', '500 Werbevideos geschaut', 500, 'play-500'),
    ('first_withdrawal', 'Erste Auszahlung', 'Erste Auszahlung beantragt', 100, 'payout'),
    ('first_referral', 'Erster Freund', 'Ersten Freund eingeladen', 75, 'referral'),
    ('streak_7', '7-Tage-Streak', '7 Tage in Folge aktiv', 100, 'streak-7'),
    ('streak_30', '30-Tage-Streak', '30 Tage in Folge aktiv', 300, 'streak-30'),
    ('early_user', 'Early Adopter', 'Unter den ersten Nutzern', 50, 'star');

INSERT INTO missions (slug, title_de, type, target_count, reward_usdt, xp_reward) VALUES
    ('daily_watch_5', '5 Videos heute', 'daily', 5, 0.001, 20),
    ('daily_watch_15', '15 Videos heute', 'daily', 15, 0.003, 50),
    ('daily_login', 'Täglicher Login', 'daily', 1, 0.0005, 10),
    ('daily_invite', 'Freund einladen', 'daily', 1, 0.002, 30),
    ('weekly_watch_100', '100 Videos diese Woche', 'weekly', 100, 0.01, 100),
    ('weekly_invite_3', '3 Freunde diese Woche', 'weekly', 3, 0.005, 75),
    ('monthly_watch_500', '500 Videos diesen Monat', 'monthly', 500, 0.05, 250),
    ('monthly_first_withdrawal', 'Erste Auszahlung', 'monthly', 1, 0.01, 100);
