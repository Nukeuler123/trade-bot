sed -i 's/;/,/' $1 && sed -i 1d $1 && echo -n "datetime,open,high,low,close,volume" >> $1 && tac $1 >> $1-rev.csv && rm $1 && mv $1-rev.csv $1
