# Playing-card faces

52 card faces from the me.uk SVG playing cards generator (CC0 / public domain;
court designs based on 19th-century Goodall & Son). No attribution required.

Source: https://www.me.uk/cards/ · GitHub: https://github.com/revk/SVG-playing-cards

Regenerate (default Goodall style):

    curl -F "zip=Download zip file of SVG for web use" https://www.me.uk/cards/makeadeck.cgi -o cards.zip

then keep `[2-9TJQKA][SHDC].svg`, drop backs/jokers, and run `svgo` over them.
The card _back_ is not from this set — it's the app's CSS `.card-back` (teal hatch).
