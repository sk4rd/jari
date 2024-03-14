now="$(date -I"minutes")"
name="$(git config user.name)"
echo "$now $1" >> "$(git rev-parse --show-toplevel)/tracking/$name"
